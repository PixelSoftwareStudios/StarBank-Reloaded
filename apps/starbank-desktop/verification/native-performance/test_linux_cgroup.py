"""Simulate kernel-provided cgroup files in temporary directories."""

import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "linux_cgroup", Path(__file__).with_name("linux-cgroup.py")
)
cgroup = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = cgroup
SPEC.loader.exec_module(cgroup)

TOKEN = "0123456789abcdef" * 2
BASELINE = {
    "cpu.stat": "usage_usec 0\nuser_usec 0\nsystem_usec 0\n",
    "memory.current": "0\n",
    "memory.peak": "0\n",
    "cgroup.events": "populated 0\nfrozen 0\n",
    "cgroup.procs": "",
}
ROOT_CONTROLS = {
    "cgroup.controllers": "io cpu memory pids\n",
    "cgroup.subtree_control": "cpu memory\n",
    "cgroup.procs": "19\n",
}


class LinuxCgroupTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        for name, value in ROOT_CONTROLS.items():
            (self.root / name).write_text(value, encoding="ascii")
        self.root_fd = self.enterContext(cgroup.directory(self.root))
        self.leaf_path = self.root / (cgroup.PREFIX + TOKEN)
        self.initial_controls = BASELINE.copy()
        mkdir, rmdir = os.mkdir, os.rmdir

        def kernel_mkdir(path, mode, *, dir_fd=None):
            self.assertEqual((path, dir_fd), (self.leaf_path.name, self.root_fd))
            mkdir(path, mode, dir_fd=dir_fd)
            for name, value in self.initial_controls.items():
                (self.leaf_path / name).write_text(value, encoding="ascii")

        def kernel_rmdir(path, *, dir_fd=None):
            self.assertEqual((path, dir_fd), (self.leaf_path.name, self.root_fd))
            for control in self.leaf_path.iterdir():
                self.assertIn(control.name, BASELINE)
                control.unlink()
            rmdir(path, dir_fd=dir_fd)

        patches = (
            patch.object(cgroup.secrets, "token_hex", return_value=TOKEN),
            patch.object(cgroup.os, "mkdir", side_effect=kernel_mkdir),
            patch.object(cgroup.os, "rmdir", side_effect=kernel_rmdir),
        )
        for replacement in patches:
            self.enterContext(replacement)

    def create(self):
        result = cgroup.setup(self.root_fd)
        self.assertEqual(
            result["baseline"],
            {
                "cpu_usage_usec": 0,
                "memory_current_bytes": 0,
                "memory_peak_bytes": 0,
                "populated": False,
            },
        )
        return cgroup.parse_leaf(json.dumps(result["leaf"]))

    def set_control(self, name, contents):
        (self.leaf_path / name).write_text(contents, encoding="ascii")

    def test_samples_owned_leaf_and_removes_it_only_after_empty(self):
        handle = self.create()
        self.set_control("cpu.stat", "usage_usec 1250\nuser_usec 1000\nsystem_usec 250\n")
        self.set_control("memory.current", "12288\n")
        self.set_control("memory.peak", "16384\n")
        self.set_control("cgroup.events", "populated 1\nfrozen 0\n")
        self.set_control("cgroup.procs", "41\n")
        self.assertEqual(
            cgroup.sample(self.root_fd, handle),
            {
                "cpu_usage_usec": 1250,
                "memory_current_bytes": 12288,
                "memory_peak_bytes": 16384,
                "populated": True,
            },
        )
        with self.assertRaisesRegex(ValueError, "populated"):
            cgroup.cleanup(self.root_fd, handle)
        self.assertTrue(self.leaf_path.is_dir())
        self.set_control("cgroup.events", "populated 0\nfrozen 0\n")
        with self.assertRaisesRegex(ValueError, "populated"):
            cgroup.cleanup(self.root_fd, handle)
        self.set_control("cgroup.procs", "")
        cgroup.cleanup(self.root_fd, handle)
        self.assertFalse(self.leaf_path.exists())
        self.assertEqual(
            {path.name: path.read_text(encoding="ascii") for path in self.root.iterdir()},
            ROOT_CONTROLS,
        )

    def test_refuses_disabled_root_controller_without_creating_leaf(self):
        control = self.root / "cgroup.subtree_control"
        control.write_text("cpu\n", encoding="ascii")
        with self.assertRaisesRegex(ValueError, "root must enable memory"):
            cgroup.setup(self.root_fd)
        self.assertFalse(self.leaf_path.exists())
        self.assertEqual(control.read_text(encoding="ascii"), "cpu\n")

    def test_existing_name_is_never_reused(self):
        handle = self.create()
        self.set_control("memory.peak", "4096\n")
        with self.assertRaises(FileExistsError):
            cgroup.setup(self.root_fd)
        self.assertEqual(cgroup.sample(self.root_fd, handle)["memory_peak_bytes"], 4096)

    def test_missing_peak_removes_the_exclusively_created_empty_leaf(self):
        del self.initial_controls["memory.peak"]
        with self.assertRaisesRegex(FileNotFoundError, "memory.peak"):
            cgroup.setup(self.root_fd)
        self.assertFalse(self.leaf_path.exists())

    def test_nonzero_baseline_is_not_published(self):
        self.initial_controls["cpu.stat"] = "usage_usec 400\n"
        with self.assertRaisesRegex(ValueError, "baseline is nonzero"):
            cgroup.setup(self.root_fd)
        self.assertFalse(self.leaf_path.exists())

    def test_setup_never_removes_populated_leaf_on_failure(self):
        self.initial_controls["cgroup.events"] = "populated 1\n"
        self.initial_controls["cgroup.procs"] = "41\n"
        with self.assertRaisesRegex(ValueError, r"Retained starbank-perf-.*populated"):
            cgroup.setup(self.root_fd)
        self.assertEqual((self.leaf_path / "cgroup.procs").read_text(), "41\n")

    def test_replacement_leaf_is_not_sampled_or_removed(self):
        handle = self.create()
        original = self.leaf_path.rename(self.root / "original")
        cgroup.setup(self.root_fd)
        for command in (cgroup.sample, cgroup.cleanup):
            with self.subTest(command=command.__name__):
                with self.assertRaisesRegex(ValueError, "identity changed"):
                    command(self.root_fd, handle)
        self.assertTrue(original.is_dir())
        self.assertTrue(self.leaf_path.is_dir())

    def test_symlink_cannot_redirect_leaf_sample_or_cleanup(self):
        handle = self.create()
        original = self.leaf_path.rename(self.root / "original")
        self.leaf_path.symlink_to(original, target_is_directory=True)
        for command in (cgroup.sample, cgroup.cleanup):
            with self.subTest(command=command.__name__):
                with self.assertRaises(OSError):
                    command(self.root_fd, handle)
        self.assertTrue(self.leaf_path.is_symlink())
        self.assertTrue(original.is_dir())

    def test_rejects_invalid_kernel_counters(self):
        handle = self.create()
        for name, contents in (
            ("cpu.stat", "usage_usec 18446744073709551616\n"),
            ("cpu.stat", "usage_usec 1\nusage_usec 2\n"),
            ("cpu.stat", "user_usec 1\n"),
            ("cpu.stat", "0" * (cgroup.MAX_CONTROL_BYTES + 1)),
            ("memory.current", "-1\n"),
            ("memory.peak", "not a counter\n"),
            ("cgroup.events", "populated 2\n"),
        ):
            with self.subTest(name=name, contents=contents[:80]):
                self.set_control(name, contents)
                with self.assertRaises(ValueError):
                    cgroup.sample(self.root_fd, handle)
                self.set_control(name, BASELINE[name])

    def test_keeps_independently_read_current_and_peak_values(self):
        handle = self.create()
        self.set_control("memory.current", "8192\n")
        self.set_control("memory.peak", "4096\n")
        measured = cgroup.sample(self.root_fd, handle)
        self.assertEqual(measured["memory_current_bytes"], 8192)
        self.assertEqual(measured["memory_peak_bytes"], 4096)

    def test_rejects_untyped_and_duplicate_handle_fields(self):
        handle = self.create()
        leaf = handle.to_json_fields()
        for changes in (
            {"token": "../original"},
            {"token": handle.token.upper()},
            {"inode": "0"},
            {"inode": 1},
            {"inode": "01"},
            {"device": True},
            {"extra": 1},
        ):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                cgroup.parse_leaf(json.dumps(leaf | changes))
        duplicate = json.dumps(leaf)[:-1] + ',"inode": 999}'
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            cgroup.parse_leaf(duplicate)

    def test_handle_retains_inode_identity_beyond_javascript_number_range(self):
        leaf = cgroup.Leaf(TOKEN, 1, cgroup.U64_MAX)
        wire = json.dumps(leaf.to_json_fields())
        self.assertIn('"inode": "18446744073709551615"', wire)
        self.assertEqual(cgroup.parse_leaf(wire), leaf)


if __name__ == "__main__":
    unittest.main()
