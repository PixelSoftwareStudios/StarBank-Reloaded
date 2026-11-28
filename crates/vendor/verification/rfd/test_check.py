"""Exercise the RFD source-integrity checks with temporary package fixtures."""

import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest


def sha256(contents):
    return hashlib.sha256(contents).hexdigest()


class IntegrityCheckTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.scratch = Path(temporary.name).resolve()
        verification = self.scratch / "verification/rfd"
        verification.mkdir(parents=True)
        self.checker = verification / "check.py"
        shutil.copyfile(Path(__file__).with_name("check.py"), self.checker)
        self.manifest_path = verification / "manifest.json"
        self.source = self.scratch / "rfd"
        self.archive = self.scratch / "rfd-fixture.crate"

        self.patch_path = "src/dialog.rs"
        self.original = b"destroy();\n"
        self.patched = b"destroy();\nunref();\n"
        self.stock = {
            "README.md": b"fixture documentation\n",
            "LICENSE": b"publisher notice\n",
            self.patch_path: self.original,
        }
        for name, contents in self.stock.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(self.patched if name == self.patch_path else contents)

        self.manifest = {
            "archive": {"name": self.archive.name},
            "files": {name: sha256(contents) for name, contents in self.stock.items()},
            "patch": {
                "path": self.patch_path,
                "before": self.original.decode(),
                "after": self.patched.decode(),
                "sha256": sha256(self.patched),
            },
        }
        self.write_archive(self.stock)

    def write_archive(self, files):
        with tarfile.open(self.archive, "w:gz") as archive:
            for name, contents in files.items():
                entry = tarfile.TarInfo(f"rfd-fixture/{name}")
                entry.size = len(contents)
                archive.addfile(entry, io.BytesIO(contents))
        self.manifest["archive"]["sha256"] = sha256(self.archive.read_bytes())
        self.write_manifest()

    def write_manifest(self):
        self.manifest_path.write_text(json.dumps(self.manifest), encoding="utf-8")

    def run_check(self, *, archive=False):
        command = [sys.executable, str(self.checker)]
        if archive:
            command.extend(["--original-crate", str(self.archive)])
        return subprocess.run(command, capture_output=True, text=True, timeout=10)

    def assert_rejected(self, message, **options):
        result = self.run_check(**options)
        self.assertEqual(result.returncode, 1, result)
        self.assertTrue(result.stderr.startswith("RFD integrity check failed:"), result)
        self.assertIn(message, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_accepts_exact_tree_with_or_without_original_crate(self):
        for archive in (False, True):
            result = self.run_check(archive=archive)
            self.assertEqual(result.returncode, 0, result)
            self.assertEqual(result.stdout, "Verified 3 vendored RFD files\n")

    def test_rejects_changed_unpatched_file(self):
        (self.source / "README.md").write_bytes(b"changed documentation\n")
        self.assert_rejected("SHA-256 mismatch: README.md")

    def test_rejects_reverted_ownership_patch(self):
        (self.source / self.patch_path).write_bytes(self.original)
        self.assert_rejected(f"SHA-256 mismatch: {self.patch_path}")

    def test_rejects_extra_file(self):
        (self.source / "src/unexpected.rs").touch()
        self.assert_rejected("unexpected in src: unexpected.rs")

    def test_rejects_missing_file(self):
        (self.source / self.patch_path).unlink()
        self.assert_rejected("missing in src: dialog.rs")

    def test_rejects_changed_publisher_license(self):
        (self.source / "LICENSE").write_bytes(b"changed notice\n")
        self.assert_rejected("SHA-256 mismatch: LICENSE")

    def test_rejects_symlink_to_identical_publisher_license(self):
        path = self.source / "LICENSE"
        target = path.rename(self.scratch / path.name)
        path.symlink_to(target)
        self.assert_rejected("LICENSE")

    def test_rejects_wrong_original_archive_digest(self):
        self.archive.write_bytes(b"another archive")
        self.assert_rejected("original crate SHA-256 mismatch", archive=True)

    def test_rejects_unlisted_archive_member(self):
        self.write_archive({**self.stock, "extra": b"extra bytes"})
        self.assert_rejected(
            "unexpected archive entry: rfd-fixture/extra", archive=True
        )

    def test_rejects_additional_change_even_with_replaced_patched_digest(self):
        altered = self.patched + b"unrelated change\n"
        (self.source / self.patch_path).write_bytes(altered)
        self.manifest["patch"]["sha256"] = sha256(altered)
        self.write_manifest()
        self.assert_rejected(
            f"ownership patch SHA-256 mismatch: {self.patch_path}", archive=True
        )


if __name__ == "__main__":
    unittest.main()
