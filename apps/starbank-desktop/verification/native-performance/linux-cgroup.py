"""Manage one fresh cgroup v2 leaf for hosted Linux resource measurements.

Setup requires an existing root with memory accounting enabled. Pass its leaf
handle as JSON to sample, members, and cleanup. App admission and timing belong
to the measurement harness.
"""

import argparse
from contextlib import ExitStack, contextmanager
from dataclasses import asdict, dataclass
import json
import os
from pathlib import Path
import re
import secrets
import stat
import sys


ROOT = Path("/sys/fs/cgroup")
PROC_ROOT = Path("/proc")
PREFIX = "starbank-perf-"
TOKEN_BYTES = 16
TOKEN = re.compile(r"[0-9a-f]{32}")
UNSIGNED = re.compile(r"0|[1-9][0-9]{0,19}")
U64_MAX = (1 << 64) - 1
MAX_CONTROL_BYTES = 4_096
MAX_MEMBER_COUNT = 256
MAX_PID = (1 << 31) - 1
MAX_UID = (1 << 32) - 2
MAX_PROC_STATUS_BYTES = 64 * 1024
MAX_EXE_BYTES = 4_096
# The /proc/<pid>/stat tail starts at state (field 3).
USER_CPU_INDEX = 14 - 3
SYSTEM_CPU_INDEX = 15 - 3
STARTTIME_INDEX = 22 - 3
MIN_STAT_TAIL_FIELDS = 52 - 2
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


@dataclass(frozen=True)
class ProcessStat:
    starttime_ticks: int
    user_cpu_ticks: int
    system_cpu_ticks: int


@contextmanager
def directory(path, *, parent=None):
    descriptor = os.open(path, DIR_FLAGS, dir_fd=parent)
    try:
        yield descriptor
    finally:
        os.close(descriptor)


def read_bounded(parent, name, limit):
    descriptor = os.open(name, FILE_FLAGS, dir_fd=parent)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError(f"Expected a regular kernel file: {name}")
        contents = stream.read(limit + 1)
    if len(contents) > limit:
        raise ValueError(f"Oversized kernel file: {name}")
    return contents


def control(leaf, name):
    return read_bounded(leaf, name, MAX_CONTROL_BYTES).decode("ascii")


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


def member_pids(leaf):
    contents = control(leaf, "cgroup.procs")
    if contents and not contents.endswith("\n"):
        raise ValueError("Incomplete cgroup.procs record")
    entries = contents[:-1].split("\n") if contents else []
    if len(entries) > MAX_MEMBER_COUNT:
        raise ValueError("Cgroup member count exceeds the snapshot limit")
    pids = {uint(entry, "cgroup.procs PID") for entry in entries}
    if any(not 0 < pid <= MAX_PID for pid in pids):
        raise ValueError("Invalid PID in cgroup.procs")
    return pids


def verify_member(process, pid, expected):
    if control(process, "cgroup") != expected:
        raise ValueError(f"PID {pid} is outside the identified cgroup leaf")


def process_stat(process, pid):
    contents = read_bounded(process, "stat", MAX_CONTROL_BYTES)
    # The parenthesized process name can itself contain spaces and parentheses.
    prefix = f"{pid} (".encode("ascii")
    tail = contents.rfind(b") ")
    if not contents.startswith(prefix) or tail < len(prefix) or not contents.endswith(b"\n"):
        raise ValueError(f"Invalid stat identity for PID {pid}")
    fields = contents[tail + 2:].split()
    if len(fields) < MIN_STAT_TAIL_FIELDS or len(fields[0]) != 1:
        raise ValueError(f"Missing stat starttime for PID {pid}")
    return ProcessStat(
        starttime_ticks=uint(
            fields[STARTTIME_INDEX].decode("ascii"), f"PID {pid} starttime"
        ),
        user_cpu_ticks=uint(fields[USER_CPU_INDEX].decode("ascii"), f"PID {pid} utime"),
        system_cpu_ticks=uint(fields[SYSTEM_CPU_INDEX].decode("ascii"), f"PID {pid} stime"),
    )


def effective_uid(process, pid):
    contents = read_bounded(process, "status", MAX_PROC_STATUS_BYTES)
    fields = [line[4:].split() for line in contents.splitlines() if line.startswith(b"Uid:")]
    if len(fields) != 1 or len(fields[0]) != 4:
        raise ValueError(f"Invalid Uid field for PID {pid}")
    ids = [uint(text.decode("ascii"), f"PID {pid} Uid") for text in fields[0]]
    if any(uid > MAX_UID for uid in ids):
        raise ValueError(f"Invalid Uid range for PID {pid}")
    return ids[1]


def executable_path(process, pid):
    path = os.readlink("exe", dir_fd=process)
    if (
        not path.startswith("/")
        or path.endswith(" (deleted)")
        or len(path.encode("utf-8")) > MAX_EXE_BYTES
        or any(ord(char) < 32 or ord(char) == 127 for char in path)
    ):
        raise ValueError(f"Invalid executable path for PID {pid}")
    return path


def members(root, handle, *, proc_root=PROC_ROOT):
    """Report each member's first stat read with subsequent identity checks."""
    with open_leaf(root, handle) as leaf:
        pids = member_pids(leaf)
        observed = []
        if pids:
            expected = f"0::/{handle.name}\n"
            with directory(proc_root) as proc, ExitStack() as process_directories:
                snapshots = []
                for pid in sorted(pids):
                    process = process_directories.enter_context(
                        directory(str(pid), parent=proc)
                    )
                    verify_member(process, pid, expected)
                    observation = process_stat(process, pid)
                    observed.append({
                        "pid": str(pid),
                        "starttime_ticks": str(observation.starttime_ticks),
                        "executable": executable_path(process, pid),
                        "effective_uid": str(effective_uid(process, pid)),
                        "user_cpu_ticks": str(observation.user_cpu_ticks),
                        "system_cpu_ticks": str(observation.system_cpu_ticks),
                    })
                    snapshots.append((pid, process, observation))

                for pid, process, observation in snapshots:
                    current = process_stat(process, pid)
                    if current.starttime_ticks != observation.starttime_ticks:
                        raise ValueError(f"Process identity changed for PID {pid}")
                    verify_member(process, pid, expected)
                    if (
                        current.user_cpu_ticks < observation.user_cpu_ticks
                        or current.system_cpu_ticks < observation.system_cpu_ticks
                    ):
                        raise ValueError(f"Process CPU ticks decreased for PID {pid}")

        if member_pids(leaf) != pids or populated(leaf) != bool(pids):
            raise ValueError(f"Cgroup membership changed while reading {handle.name}")
        verify_identity(os.stat(handle.name, dir_fd=root, follow_symlinks=False), handle)
    return {"members": observed}


def cleanup(root, handle):
    with open_leaf(root, handle) as leaf:
        remove_empty(root, leaf, handle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("setup")
    for command in ("sample", "members", "cleanup"):
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
            elif args.command == "members":
                print(json.dumps(members(root, args.leaf)))
            else:
                cleanup(root, args.leaf)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Linux cgroup measurement failed: {error}\n")


if __name__ == "__main__":
    main()
