"""Verify the pinned RFD release and its GTK ownership patches."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import stat
import tarfile

VERIFICATION = Path(__file__).absolute().parent
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
FILE_FLAGS = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW
ROOT = Path(".")


@contextmanager
def open_directory(path):
    path = path.absolute()
    if ".." in path.parts:
        raise ValueError(f"parent components are not allowed in {path}")

    descriptor = os.open(path.anchor, DIR_FLAGS)
    try:
        for component in path.parts[1:]:
            child = os.open(component, DIR_FLAGS, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        yield descriptor
    finally:
        os.close(descriptor)


@contextmanager
def open_regular(directory, name):
    descriptor = os.open(name, FILE_FLAGS, dir_fd=directory)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ValueError(f"expected regular file: {name}")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            yield stream
    finally:
        os.close(descriptor)


def directories_for(files):
    directories = {}
    for name in files:
        path = Path(name)
        if (
            path.is_absolute()
            or path == ROOT
            or ".." in path.parts
            or path.as_posix() != name
        ):
            raise ValueError(f"invalid manifest path: {name}")
        while path != ROOT:
            directories.setdefault(path.parent, set()).add(path.name)
            path = path.parent
    if any(path.as_posix() in files for path in directories):
        raise ValueError("a manifest file is also a directory")
    return directories


def patches_for(stock, entries):
    patches = {}
    for entry in entries:
        path = entry["path"]
        if path not in stock:
            raise ValueError(f"unexpected ownership patch path: {path}")
        if path in patches:
            raise ValueError(f"duplicate ownership patch path: {path}")
        if entry["sha256"] == stock[path]:
            raise ValueError(f"ownership patch must change the stock file: {path}")

        hunks = []
        for hunk in entry["hunks"]:
            if (
                not isinstance(hunk, dict)
                or set(hunk) != {"before", "after"}
                or not all(isinstance(value, str) for value in hunk.values())
            ):
                raise ValueError(f"invalid ownership hunk fields: {path}")
            before, after = (hunk[key].encode("utf-8") for key in ("before", "after"))
            if not before or not after or before == after:
                raise ValueError(f"invalid ownership hunk bytes: {path}")
            if any(before == old or after == new for old, new in hunks):
                raise ValueError(f"duplicate ownership hunk: {path}")
            hunks.append((before, after))
        if not hunks:
            raise ValueError(f"missing ownership hunks: {path}")
        patches[path] = (entry["sha256"], hunks)
    if not patches:
        raise ValueError("missing ownership patches")
    return patches


def apply_hunks(contents, hunks, path, *, reverse=False):
    edits = []
    for before, after in hunks:
        if reverse:
            before, after = after, before
        offset = contents.find(before)
        if offset == -1 or contents.find(before, offset + 1) != -1:
            raise ValueError(f"ownership hunk must match exactly once: {path}")
        edits.append((offset, offset + len(before), after))

    result = bytearray()
    previous_end = 0
    for start, end, replacement in sorted(edits):
        if start < previous_end:
            raise ValueError(f"overlapping ownership hunks: {path}")
        result.extend(contents[previous_end:start])
        result.extend(replacement)
        previous_end = end
    result.extend(contents[previous_end:])
    return result


def verify_tree(directory, path, stock, patches, directories):
    expected = directories[path]
    with os.scandir(directory) as entries:
        actual = {entry.name for entry in entries}

    if missing := sorted(expected - actual):
        raise ValueError(f"missing in {path}: {', '.join(missing)}")
    if extra := sorted(actual - expected):
        raise ValueError(f"unexpected in {path}: {', '.join(extra)}")

    for name in sorted(expected):
        child = path / name
        if child in directories:
            descriptor = os.open(name, DIR_FLAGS, dir_fd=directory)
            try:
                verify_tree(descriptor, child, stock, patches, directories)
            finally:
                os.close(descriptor)
        else:
            with open_regular(directory, name) as stream:
                stock_hash = stock[child.as_posix()]
                patch = patches.get(child.as_posix())
                expected_hash = stock_hash if patch is None else patch[0]
                actual_hash = hashlib.file_digest(stream, "sha256").hexdigest()
                if actual_hash != expected_hash:
                    raise ValueError(f"SHA-256 mismatch: {child}")
                if patch is not None:
                    stream.seek(0)
                    original = apply_hunks(stream.read(), patch[1], child, reverse=True)
                    if hashlib.sha256(original).hexdigest() != stock_hash:
                        raise ValueError(f"stock SHA-256 mismatch after reversal: {child}")
                    verify_patch(original, child, patch)


def verify_patch(stock_bytes, path, patch):
    expected_hash, hunks = patch
    patched = apply_hunks(stock_bytes, hunks, path)
    if hashlib.sha256(patched).hexdigest() != expected_hash:
        raise ValueError(f"ownership patch SHA-256 mismatch: {path}")


def verify_archive(archive_path, archive, stock, patches):
    prefix = archive["name"].removesuffix(".crate") + "/"
    remaining = set(stock)
    with open_directory(archive_path.parent) as directory:
        with open_regular(directory, archive_path.name) as stream:
            archive_hash = hashlib.file_digest(stream, "sha256").hexdigest()
            if archive_hash != archive["sha256"]:
                raise ValueError("original crate SHA-256 mismatch")
            stream.seek(0)
            with tarfile.open(fileobj=stream, mode="r:gz") as package:
                for entry in package:
                    name = entry.name.removeprefix(prefix)
                    if (
                        not entry.name.startswith(prefix)
                        or name not in remaining
                        or not entry.isfile()
                    ):
                        raise ValueError(f"unexpected archive entry: {entry.name}")
                    with package.extractfile(entry) as member:
                        contents = member.read()
                    if hashlib.sha256(contents).hexdigest() != stock[name]:
                        raise ValueError(f"baseline SHA-256 mismatch: {name}")
                    if name in patches:
                        verify_patch(contents, name, patches[name])
                    remaining.remove(name)
    if remaining:
        raise ValueError(f"missing archive files: {', '.join(sorted(remaining))}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=VERIFICATION.parents[1] / "rfd")
    parser.add_argument("--original-crate", type=Path)
    args = parser.parse_args()

    try:
        manifest_path = VERIFICATION / "manifest.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        stock = manifest["files"]
        patches = patches_for(stock, manifest["patches"])
        directories = directories_for(stock)
        if args.original_crate is not None:
            verify_archive(args.original_crate, manifest["archive"], stock, patches)
        with open_directory(args.source) as directory:
            verify_tree(directory, ROOT, stock, patches, directories)
    except (KeyError, OSError, TypeError, ValueError, tarfile.TarError) as error:
        parser.exit(1, f"RFD integrity check failed: {error}\n")

    print(f"Verified {len(stock)} vendored RFD files")


if __name__ == "__main__":
    main()
