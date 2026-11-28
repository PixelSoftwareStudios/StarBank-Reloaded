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
APP_PATH = "/usr/bin/starbank-desktop"
WEBKIT_PATH = "/usr/lib/webkit2gtk-4.1/WebKitWebProcess"
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
        self.proc_root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        for name, value in ROOT_CONTROLS.items():
            (self.root / name).write_text(value, encoding="ascii")
        self.root_fd = self.enterContext(cgroup.directory(self.root))
        self.leaf_path = self.root / (cgroup.PREFIX + TOKEN)
        self.initial_controls = BASELINE.copy()
        mkdir, rmdir = os.mkdir, os.rmdir
        self.mkdir_process_fixture = mkdir

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

    def set_members(self, pids):
        self.set_control("cgroup.procs", "".join(f"{pid}\n" for pid in pids))
        self.set_control("cgroup.events", f"populated {int(bool(pids))}\nfrozen 0\n")

    def snapshot_members(self, handle):
        return cgroup.members(self.root_fd, handle, proc_root=self.proc_root)

    def process_fixture(
        self, handle, pid, executable=APP_PATH, starttime=87, *,
        user_cpu=0, system_cpu=0,
    ):
        process = self.proc_root / str(pid)
        self.mkdir_process_fixture(process)
        # Include an internal ') ' and a byte outside ASCII in the comm field.
        fields = [b"S", *([b"0"] * 18)]
        # The two following fields describe waited-for children, not this process.
        fields[11:15] = [str(user_cpu).encode(), str(system_cpu).encode(), b"314", b"159"]
        before = b" ".join(fields)
        tail = b" 0" * 30
        (process / "stat").write_bytes(
            f"{pid} (name".encode("ascii") + b") odd\xff) " + before
            + f" {starttime}".encode("ascii") + tail + b"\n"
        )
        (process / "status").write_bytes(b"Name:\ttest\xff\nUid:\t1000\t1001\t1002\t1003\n")
        (process / "cgroup").write_text(f"0::/{handle.name}\n", encoding="ascii")
        (process / "exe").symlink_to(executable)
        return process

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

    def test_members_only_reads_listed_processes_and_deduplicates(self):
        handle = self.create()
        first, second, unrelated = 41, 42, 43
        self.set_members([second, first, second])
        self.process_fixture(
            handle, first, starttime=(1 << 53) + 1,
            user_cpu=(1 << 53) + 3, system_cpu=(1 << 53) + 5,
        )
        self.process_fixture(handle, second, WEBKIT_PATH, user_cpu=12, system_cpu=9)
        # Missing metadata for this unrelated PID must have no effect.
        self.mkdir_process_fixture(self.proc_root / str(unrelated))
        self.assertEqual(
            self.snapshot_members(handle),
            {
                "members": [
                    {
                        "pid": "41",
                        "starttime_ticks": "9007199254740993",
                        "executable": APP_PATH,
                        "effective_uid": "1001",
                        "user_cpu_ticks": "9007199254740995",
                        "system_cpu_ticks": "9007199254740997",
                    },
                    {
                        "pid": "42",
                        "starttime_ticks": "87",
                        "executable": WEBKIT_PATH,
                        "effective_uid": "1001",
                        "user_cpu_ticks": "12",
                        "system_cpu_ticks": "9",
                    },
                ],
            },
        )

    def test_members_empty_when_leaf_is_empty_and_refuses_partial_membership(self):
        handle = self.create()
        self.assertEqual(self.snapshot_members(handle), {"members": []})
        self.set_control("cgroup.events", "populated 1\nfrozen 0\n")
        with self.assertRaisesRegex(ValueError, "membership changed"):
            self.snapshot_members(handle)

    def test_members_refuses_malformed_or_excessive_pid_lists(self):
        handle = self.create()
        invalid = (
            "0\n", "2147483648\n", "-5\n", "01\n", "+4\n", "4\r\n", "4",
            "../41\n", "41 42\n", "41\n" * (cgroup.MAX_MEMBER_COUNT + 1),
        )
        for contents in invalid:
            with self.subTest(contents=contents[:80]):
                self.set_control("cgroup.procs", contents)
                with self.assertRaises(ValueError):
                    self.snapshot_members(handle)

    def test_members_rejects_outside_process_before_other_metadata_reads(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        process = self.process_fixture(handle, pid)
        (process / "cgroup").write_text("0::/unrelated\n", encoding="ascii")
        (process / "stat").unlink()
        with self.assertRaisesRegex(ValueError, "outside the identified cgroup leaf"):
            self.snapshot_members(handle)

    def test_members_rejects_missing_or_symlinked_process_directory(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        with self.assertRaises(FileNotFoundError):
            self.snapshot_members(handle)
        process = self.process_fixture(handle, pid)
        renamed = process.rename(self.proc_root / "unrelated")
        process.symlink_to(renamed, target_is_directory=True)
        with self.assertRaises(OSError):
            self.snapshot_members(handle)

    def test_members_rejects_nonmatching_stat_identity_and_malformed_uid(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        process = self.process_fixture(handle, pid)
        status = process / "status"
        original = status.read_bytes()
        for contents in (
            b"Uid:\t1000\t1001\t1002\n",
            b"Uid:\t1000\t4294967295\t1002\t1003\n",
            original + b"Uid:\t1000\t1009\t1002\t1003\n",
        ):
            with self.subTest(contents=contents), self.assertRaisesRegex(ValueError, "Uid"):
                status.write_bytes(contents)
                self.snapshot_members(handle)
        status.write_bytes(original)
        stat = process / "stat"
        stat.write_bytes(stat.read_bytes().replace(b"41 (", b"52 (", 1))
        with self.assertRaisesRegex(ValueError, "Invalid stat identity"):
            self.snapshot_members(handle)

    def test_members_rejects_abnormal_executable_links(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        process = self.process_fixture(handle, pid)
        executable = process / "exe"
        for target in ("starbank-desktop", APP_PATH + " (deleted)", "/name\ninjected"):
            with self.subTest(target=target):
                executable.unlink()
                executable.symlink_to(target)
                with self.assertRaisesRegex(ValueError, "Invalid executable path"):
                    self.snapshot_members(handle)

    def test_members_rejects_pid_reuse_between_metadata_passes(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        process = self.process_fixture(handle, pid)
        stat = process / "stat"
        readlink = os.readlink

        def replace_process(name, *, dir_fd):
            result = readlink(name, dir_fd=dir_fd)
            stat.write_bytes(stat.read_bytes().replace(b" 87 ", b" 99 ", 1))
            return result

        with patch.object(cgroup.os, "readlink", side_effect=replace_process):
            with self.assertRaisesRegex(ValueError, "Process identity changed"):
                self.snapshot_members(handle)

    def test_members_rejects_malformed_process_cpu_ticks(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        stat = self.process_fixture(handle, pid, user_cpu=500, system_cpu=250) / "stat"
        original = stat.read_bytes()
        for contents, field in (
            (b"-1 250", "utime"),
            (b"01 250", "utime"),
            (b"500 not_ticks", "stime"),
            (b"500 18446744073709551616", "stime"),
        ):
            with self.subTest(contents=contents), self.assertRaisesRegex(ValueError, field):
                stat.write_bytes(original.replace(b"500 250", contents))
                self.snapshot_members(handle)

    def test_members_keeps_initial_cpu_ticks_and_rejects_a_decrease(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        stat = self.process_fixture(handle, pid, user_cpu=500, system_cpu=250) / "stat"
        original = stat.read_bytes()
        readlink = os.readlink

        def change_ticks(reading):
            def after_first_stat(name, *, dir_fd):
                path = readlink(name, dir_fd=dir_fd)
                stat.write_bytes(original.replace(b"500 250", reading))
                return path
            return after_first_stat

        with patch.object(cgroup.os, "readlink", side_effect=change_ticks(b"505 260")):
            observed = self.snapshot_members(handle)["members"][0]
        self.assertEqual(observed["user_cpu_ticks"], "500")
        self.assertEqual(observed["system_cpu_ticks"], "250")

        for reading in (b"499 270", b"505 249"):
            with self.subTest(reading=reading):
                stat.write_bytes(original)
                with patch.object(cgroup.os, "readlink", side_effect=change_ticks(reading)):
                    with self.assertRaisesRegex(ValueError, "Process CPU ticks decreased"):
                        self.snapshot_members(handle)

    def test_members_rejects_process_migration_or_group_membership_change(self):
        handle = self.create()
        pid = 41
        self.set_members([pid])
        process = self.process_fixture(handle, pid)
        readlink = os.readlink

        def migrate_process(name, *, dir_fd):
            result = readlink(name, dir_fd=dir_fd)
            (process / "cgroup").write_text("0::/unrelated\n", encoding="ascii")
            return result

        with patch.object(cgroup.os, "readlink", side_effect=migrate_process):
            with self.assertRaisesRegex(ValueError, "outside the identified cgroup leaf"):
                self.snapshot_members(handle)
        (process / "cgroup").write_text(f"0::/{handle.name}\n", encoding="ascii")

        def add_group_member(name, *, dir_fd):
            result = readlink(name, dir_fd=dir_fd)
            self.set_members([pid, 42])
            return result

        with patch.object(cgroup.os, "readlink", side_effect=add_group_member):
            with self.assertRaisesRegex(ValueError, "membership changed"):
                self.snapshot_members(handle)

    def test_members_rejects_replaced_leaf_identity_before_proc_reads(self):
        handle = self.create()
        self.leaf_path.rename(self.root / "original")
        cgroup.setup(self.root_fd)
        self.set_members([41])
        with self.assertRaisesRegex(ValueError, "identity changed"):
            self.snapshot_members(handle)


if __name__ == "__main__":
    unittest.main()
