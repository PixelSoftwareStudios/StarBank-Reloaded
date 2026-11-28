"""Manage one fresh cgroup v2 leaf for hosted Linux resource measurements.

Setup requires an existing root with memory accounting enabled. Pass its leaf
handle as JSON to sample and cleanup. App admission and timing belong to the
measurement harness.
"""

import argparse
from contextlib import contextmanager
from dataclasses import asdict, dataclass
import json
import os
from pathlib import Path
import re
import secrets
import stat
import sys


ROOT = Path("/sys/fs/cgroup")
PREFIX = "starbank-perf-"
TOKEN_BYTES = 16
TOKEN = re.compile(r"[0-9a-f]{32}")
UNSIGNED = re.compile(r"0|[1-9][0-9]{0,19}")
U64_MAX = (1 << 64) - 1
MAX_CONTROL_BYTES = 4_096
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
FILE_FLAGS = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW


@dataclass(frozen=True)
class Leaf:
    token: str
    device: int
    inode: int

    def __post_init__(self):
        if (
            not isinstance(self.token, str)
            or not TOKEN.fullmatch(self.token)
            or type(self.device) is not int
            or not 0 <= self.device <= U64_MAX
            or type(self.inode) is not int
            or not 0 < self.inode <= U64_MAX
        ):
            raise ValueError("Invalid cgroup leaf handle")

    @property
    def name(self):
        return PREFIX + self.token

    def to_json_fields(self):
        # Device and inode numbers can exceed JSON's exact JavaScript range.
        return {"token": self.token, "device": str(self.device), "inode": str(self.inode)}


@dataclass(frozen=True)
class Counters:
    cpu_usage_usec: int
    memory_current_bytes: int
    memory_peak_bytes: int
    populated: bool


@contextmanager
def directory(path, *, parent=None):
    descriptor = os.open(path, DIR_FLAGS, dir_fd=parent)
    try:
        yield descriptor
    finally:
        os.close(descriptor)


def control(leaf, name):
    descriptor = os.open(name, FILE_FLAGS, dir_fd=leaf)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError(f"Expected a regular cgroup file: {name}")
        contents = stream.read(MAX_CONTROL_BYTES + 1)
    if len(contents) > MAX_CONTROL_BYTES:
        raise ValueError(f"Oversized cgroup file: {name}")
    return contents.decode("ascii")


def uint(text, name):
    if not UNSIGNED.fullmatch(text) or int(text) > U64_MAX:
        raise ValueError(f"Invalid unsigned cgroup value: {name}")
    return int(text)


def field(leaf, name, key):
    fields = {}
    for line in control(leaf, name).splitlines():
        parts = line.split()
        if len(parts) != 2 or parts[0] in fields:
            raise ValueError(f"Invalid cgroup fields: {name}")
        fields[parts[0]] = uint(parts[1], f"{name}.{parts[0]}")
    if key not in fields:
        raise ValueError(f"Missing cgroup field: {name}.{key}")
    return fields[key]


def populated(leaf):
    value = field(leaf, "cgroup.events", "populated")
    if value not in (0, 1):
        raise ValueError("Invalid cgroup.events.populated")
    return bool(value)


def counters(leaf):
    cpu = field(leaf, "cpu.stat", "usage_usec")
    current = uint(control(leaf, "memory.current").strip(), "memory.current")
    # The memory controller's watermark update races with concurrent charges.
    peak = uint(control(leaf, "memory.peak").strip(), "memory.peak")
    return Counters(cpu, current, peak, populated(leaf))


def unique_fields(pairs):
    fields = dict(pairs)
    if len(fields) != len(pairs):
        raise ValueError("Duplicate cgroup leaf handle fields")
    return fields


def parse_leaf(value):
    handle = json.loads(value, object_pairs_hook=unique_fields)
    if not isinstance(handle, dict) or set(handle) != {"token", "device", "inode"}:
        raise ValueError("Invalid cgroup leaf handle fields")
    device, inode = handle["device"], handle["inode"]
    if not isinstance(device, str) or not isinstance(inode, str):
        raise ValueError("Cgroup device and inode must be decimal strings")
    return Leaf(handle["token"], uint(device, "device"), uint(inode, "inode"))


def verify_identity(info, handle):
    if (info.st_dev, info.st_ino) != (handle.device, handle.inode):
        raise ValueError(f"Cgroup leaf identity changed: {handle.name}")


@contextmanager
def open_leaf(root, handle):
    with directory(handle.name, parent=root) as leaf:
        verify_identity(os.fstat(leaf), handle)
        yield leaf


def remove_empty(root, leaf, handle):
    if populated(leaf) or control(leaf, "cgroup.procs").strip():
        raise ValueError(f"Cgroup leaf is populated: {handle.name}")
    named = os.stat(handle.name, dir_fd=root, follow_symlinks=False)
    verify_identity(named, handle)
    os.rmdir(handle.name, dir_fd=root)


def setup(root):
    available = control(root, "cgroup.controllers").split()
    enabled = control(root, "cgroup.subtree_control").split()
    if "memory" not in available or "memory" not in enabled:
        raise ValueError("The existing cgroup root must enable memory accounting")

    token = secrets.token_hex(TOKEN_BYTES)
    name = PREFIX + token
    os.mkdir(name, mode=0o755, dir_fd=root)
    with directory(name, parent=root) as leaf:
        info = os.fstat(leaf)
        handle = Leaf(token, info.st_dev, info.st_ino)
        try:
            baseline = counters(leaf)
            if baseline != Counters(0, 0, 0, False):
                raise ValueError(f"Cgroup leaf baseline is nonzero: {name}")
            if control(leaf, "cgroup.procs").strip():
                raise ValueError(f"Cgroup leaf has existing processes: {name}")
        except (OSError, ValueError) as error:
            try:
                remove_empty(root, leaf, handle)
            except (OSError, ValueError) as cleanup:
                raise ValueError(f"{error}. Retained {name}: {cleanup}") from cleanup
            raise
    return {"leaf": handle.to_json_fields(), "baseline": asdict(baseline)}


def sample(root, handle):
    with open_leaf(root, handle) as leaf:
        return asdict(counters(leaf))


def cleanup(root, handle):
    with open_leaf(root, handle) as leaf:
        remove_empty(root, leaf, handle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("setup")
    for command in ("sample", "cleanup"):
        commands.add_parser(command).add_argument("leaf", type=parse_leaf)
    try:
        args = parser.parse_args()
        if sys.platform != "linux":
            raise ValueError("Cgroup accounting requires a Linux CI runner")
        with directory(ROOT) as root:
            if args.command == "setup":
                print(json.dumps(setup(root)))
            elif args.command == "sample":
                print(json.dumps(sample(root, args.leaf)))
            else:
                cleanup(root, args.leaf)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Linux cgroup measurement failed: {error}\n")


if __name__ == "__main__":
    main()
