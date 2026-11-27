"""Exercise the GLib integrity checker with small temporary fixtures."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class IntegrityCheckTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.scratch = Path(temporary.name).resolve()

        verification = self.scratch / "verification/glib"
        verification.mkdir(parents=True)
        self.checker = verification / "check.py"
        shutil.copyfile(Path(__file__).with_name("check.py"), self.checker)

        self.source = self.scratch / "glib"
        fixture = {
            "README.md": b"fixture documentation\n",
            "src/variant_iter.rs": b"patched fixture bytes\n",
        }
        for name, contents in fixture.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)

        self.archive = self.scratch / "fixture.crate"
        original = b"fixture archive bytes\n"
        self.archive.write_bytes(original)
        manifest = {
            "archive": {"sha256": hashlib.sha256(original).hexdigest()},
            "files": {
                name: hashlib.sha256(contents).hexdigest()
                for name, contents in fixture.items()
            },
        }
        (verification / "manifest.json").write_text(
            json.dumps(manifest), encoding="utf-8"
        )

    def run_check(self, *, source=None, archive=None):
        command = [sys.executable, str(self.checker)]
        if source is not None:
            command.extend(["--source", str(source)])
        archive_path = self.archive if archive is None else archive
        command.extend(["--original-crate", str(archive_path)])
        return subprocess.run(command, capture_output=True, text=True, timeout=10)

    def assert_rejected(self, message, **options):
        result = self.run_check(**options)
        self.assertEqual(result.returncode, 1, result)
        self.assertTrue(result.stderr.startswith("GLib integrity check failed:"), result)
        self.assertIn(message, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_accepts_exact_files_and_archive(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result)
        self.assertEqual(result.stdout, "Verified 2 vendored GLib files\n")

    def test_rejects_changed_file(self):
        (self.source / "README.md").write_bytes(b"changed bytes\n")
        self.assert_rejected("SHA-256 mismatch: README.md")

    def test_rejects_patched_file_replaced_with_original_bytes(self):
        (self.source / "src/variant_iter.rs").write_bytes(b"original fixture bytes\n")
        self.assert_rejected("SHA-256 mismatch: src/variant_iter.rs")

    def test_rejects_missing_file(self):
        (self.source / "README.md").unlink()
        self.assert_rejected("missing in .: README.md")

    def test_rejects_unexpected_file(self):
        (self.source / "extra.txt").touch()
        self.assert_rejected("unexpected in .: extra.txt")

    def test_rejects_unexpected_empty_directory(self):
        (self.source / "extra").mkdir()
        self.assert_rejected("unexpected in .: extra")

    def test_rejects_symlink_to_identical_file(self):
        path = self.source / "README.md"
        target = path.rename(self.scratch / path.name)
        path.symlink_to(target)
        self.assert_rejected(f"'{path.name}'")

    def test_rejects_symlink_to_identical_directory(self):
        path = self.source / "src"
        target = path.rename(self.scratch / path.name)
        path.symlink_to(target, target_is_directory=True)
        self.assert_rejected(f"'{path.name}'")

    def test_rejects_symlinked_source_root(self):
        alias = self.scratch / "alias"
        alias.symlink_to(self.source, target_is_directory=True)
        self.assert_rejected(f"'{alias.name}'", source=alias)

    def test_rejects_symlinked_source_ancestor(self):
        alias = self.scratch / "alias"
        alias.symlink_to(self.scratch, target_is_directory=True)
        self.assert_rejected(f"'{alias.name}'", source=alias / self.source.name)

    def test_rejects_named_pipe_replacing_file(self):
        path = self.source / "README.md"
        path.unlink()
        os.mkfifo(path)
        self.assert_rejected("expected regular file: README.md")

    def test_rejects_changed_archive(self):
        self.archive.write_bytes(b"changed archive bytes\n")
        self.assert_rejected("original crate SHA-256 mismatch")

    def test_rejects_symlinked_archive(self):
        alias = self.scratch / "archive-link"
        alias.symlink_to(self.archive)
        self.assert_rejected(f"'{alias.name}'", archive=alias)


if __name__ == "__main__":
    unittest.main()
