"""Verify the pinned RFD release and its GTK chooser ownership patch."""

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


def verify_tree(directory, path, files, directories):
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
                verify_tree(descriptor, child, files, directories)
            finally:
                os.close(descriptor)
        else:
            with open_regular(directory, name) as stream:
                actual_hash = hashlib.file_digest(stream, "sha256").hexdigest()
                if actual_hash != files[child.as_posix()]:
                    raise ValueError(f"SHA-256 mismatch: {child}")


def verify_archive(archive_path, archive, stock, patch):
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
                    if name == patch["path"]:
                        verify_patch(contents, patch)
                    remaining.remove(name)
    if remaining:
        raise ValueError(f"missing archive files: {', '.join(sorted(remaining))}")


def verify_patch(contents, patch):
    before = patch["before"].encode("utf-8")
    if not before or contents.count(before) != 1:
        raise ValueError("ownership patch must match exactly one stock hunk")
    patched = contents.replace(before, patch["after"].encode("utf-8"), 1)
    if hashlib.sha256(patched).hexdigest() != patch["sha256"]:
        raise ValueError(f"ownership patch SHA-256 mismatch: {patch['path']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=VERIFICATION.parents[1] / "rfd")
    parser.add_argument("--original-crate", type=Path)
    args = parser.parse_args()

    try:
        manifest_path = VERIFICATION / "manifest.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        stock = manifest["files"]
        patch = manifest["patch"]
        if stock[patch["path"]] == patch["sha256"]:
            raise ValueError("the ownership patch must change the stock file")
        files = {**stock, patch["path"]: patch["sha256"]}
        directories = directories_for(files)
        if args.original_crate is not None:
            verify_archive(args.original_crate, manifest["archive"], stock, patch)
        with open_directory(args.source) as directory:
            verify_tree(directory, ROOT, files, directories)
    except (KeyError, OSError, TypeError, ValueError, tarfile.TarError) as error:
        parser.exit(1, f"RFD integrity check failed: {error}\n")

    print(f"Verified {len(files)} vendored RFD files")


if __name__ == "__main__":
    main()
