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
        self.message_path = "src/message.rs"
        self.original = b"choose();\ndestroy();\n"
        self.patched = b"choose();\nfree();\ndestroy();\nunref();\n"
        self.message_original = b"collect();\n"
        self.message_patched = b"collect();\nfree_list();\n"
        self.stock = {
            "README.md": b"fixture documentation\n",
            "LICENSE": b"publisher notice\n",
            self.patch_path: self.original,
            self.message_path: self.message_original,
        }
        source_files = {
            **self.stock,
            self.patch_path: self.patched,
            self.message_path: self.message_patched,
        }
        for name, contents in source_files.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)

        self.chooser_patch = {
            "path": self.patch_path,
            "sha256": sha256(self.patched),
            "hunks": [
                {"before": "choose();\n", "after": "choose();\nfree();\n"},
                {"before": "destroy();\n", "after": "destroy();\nunref();\n"},
            ],
        }
        self.message_patch = {
            "path": self.message_path,
            "sha256": sha256(self.message_patched),
            "hunks": [
                {
                    "before": self.message_original.decode(),
                    "after": self.message_patched.decode(),
                },
            ],
        }
        self.manifest = {
            "archive": {"name": self.archive.name},
            "files": {name: sha256(contents) for name, contents in self.stock.items()},
            "patches": [self.chooser_patch, self.message_patch],
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
            self.assertEqual(result.stdout, "Verified 4 vendored RFD files\n")

    def test_rejects_changed_unpatched_file(self):
        (self.source / "README.md").write_bytes(b"changed documentation\n")
        self.assert_rejected("SHA-256 mismatch: README.md")

    def test_rejects_reverting_any_ownership_hunk(self):
        for patch in self.manifest["patches"]:
            name = patch["path"]
            path = self.source / name
            contents = path.read_bytes()
            for hunk in patch["hunks"]:
                with self.subTest(path=name, hunk=hunk["before"]):
                    reverted = contents.replace(
                        hunk["after"].encode(), hunk["before"].encode()
                    )
                    path.write_bytes(reverted)
                    self.assert_rejected(f"SHA-256 mismatch: {name}")
            path.write_bytes(contents)

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
        self.chooser_patch["sha256"] = sha256(altered)
        self.write_manifest()
        self.assert_rejected(f"stock SHA-256 mismatch after reversal: {self.patch_path}")
        self.assert_rejected(
            f"ownership patch SHA-256 mismatch: {self.patch_path}", archive=True
        )

    def test_rejects_missing_hunk_description(self):
        self.chooser_patch["hunks"].pop()
        self.write_manifest()
        self.assert_rejected(f"stock SHA-256 mismatch after reversal: {self.patch_path}")
        self.assert_rejected(
            f"ownership patch SHA-256 mismatch: {self.patch_path}", archive=True
        )

    def test_rejects_missing_patch_description(self):
        self.manifest["patches"].remove(self.message_patch)
        self.write_manifest()
        self.assert_rejected(f"SHA-256 mismatch: {self.message_path}")

    def test_rejects_duplicate_patch_path(self):
        self.manifest["patches"].append(self.chooser_patch)
        self.write_manifest()
        self.assert_rejected(f"duplicate ownership patch path: {self.patch_path}")

    def test_rejects_duplicate_hunk(self):
        self.chooser_patch["hunks"].append(self.chooser_patch["hunks"][0])
        self.write_manifest()
        self.assert_rejected(f"duplicate ownership hunk: {self.patch_path}")

    def test_rejects_unlisted_patch_path(self):
        self.message_patch["path"] = "src/unexpected.rs"
        self.write_manifest()
        self.assert_rejected("unexpected ownership patch path: src/unexpected.rs")

    def test_rejects_hunk_absent_from_stock_and_patched_file(self):
        self.message_patch["hunks"].append(
            {"before": "allocate();\n", "after": "allocate();\nrelease();\n"}
        )
        self.write_manifest()
        for archive in (False, True):
            self.assert_rejected(
                f"ownership hunk must match exactly once: {self.message_path}",
                archive=archive,
            )

    def test_rejects_overlapping_hunks(self):
        self.chooser_patch["hunks"][0]["before"] += "destroy();\n"
        self.chooser_patch["hunks"][0]["after"] += "destroy();\n"
        self.write_manifest()
        for archive in (False, True):
            self.assert_rejected(
                f"overlapping ownership hunks: {self.patch_path}", archive=archive
            )

    def test_rejects_ambiguous_stock_hunk_even_with_a_unique_patched_match(self):
        repeated = b"choose();\n"
        stock = self.original + repeated
        altered = self.patched + repeated
        (self.source / self.patch_path).write_bytes(altered)
        self.manifest["files"][self.patch_path] = sha256(stock)
        self.chooser_patch["sha256"] = sha256(altered)
        self.write_archive({**self.stock, self.patch_path: stock})
        for archive in (False, True):
            self.assert_rejected(
                f"ownership hunk must match exactly once: {self.patch_path}",
                archive=archive,
            )


if __name__ == "__main__":
    unittest.main()
