"""Verify the pinned GLib source snapshot on Linux and macOS."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import stat

VERIFICATION = Path(__file__).absolute().parent
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
FILE_FLAGS = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW


@contextmanager
def open_directory(path):
    path = path.absolute()
    if ".." in path.parts:
        raise ValueError(f"parent components are not allowed in {path}")

    descriptor = os.open(path.anchor, DIR_FLAGS)
    try:
        for component in path.parts[1:]:
            next_descriptor = os.open(component, DIR_FLAGS, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = next_descriptor
        yield descriptor
    finally:
        os.close(descriptor)


def digest(directory, name):
    descriptor = os.open(name, FILE_FLAGS, dir_fd=directory)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ValueError(f"expected regular file: {name}")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()
    finally:
        os.close(descriptor)


def verify_tree(directory, path, files, directories):
    with os.scandir(directory) as entries:
        actual = {entry.name for entry in entries}

    expected = directories[path]
    if missing := sorted(expected - actual):
        raise ValueError(f"missing in {path}: {', '.join(missing)}")
    if unexpected := sorted(actual - expected):
        raise ValueError(f"unexpected in {path}: {', '.join(unexpected)}")

    for name in sorted(expected):
        child = path / name
        if child in directories:
            descriptor = os.open(name, DIR_FLAGS, dir_fd=directory)
            try:
                verify_tree(descriptor, child, files, directories)
            finally:
                os.close(descriptor)
        elif digest(directory, name) != files[child]:
            raise ValueError(f"SHA-256 mismatch: {child}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=VERIFICATION.parents[1] / "glib")
    parser.add_argument("--original-crate", type=Path)
    args = parser.parse_args()

    try:
        manifest = json.loads((VERIFICATION / "manifest.json").read_text(encoding="utf-8"))
        files = {Path(name): checksum for name, checksum in manifest["files"].items()}
        directories = {}
        root = Path(".")
        for path in files:
            if path.is_absolute() or path == root or ".." in path.parts:
                raise ValueError(f"invalid manifest path: {path}")
            while path != root:
                directories.setdefault(path.parent, set()).add(path.name)
                path = path.parent

        if args.original_crate is not None:
            archive = args.original_crate
            with open_directory(archive.parent) as directory:
                if digest(directory, archive.name) != manifest["archive"]["sha256"]:
                    raise ValueError("original crate SHA-256 mismatch")

        with open_directory(args.source) as directory:
            verify_tree(directory, root, files, directories)
    except (OSError, ValueError) as error:
        parser.exit(1, f"GLib integrity check failed: {error}\n")

    print(f"Verified {len(files)} vendored GLib files")


if __name__ == "__main__":
    main()
