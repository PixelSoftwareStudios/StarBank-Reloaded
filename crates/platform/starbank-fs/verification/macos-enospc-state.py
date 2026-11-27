#!/usr/bin/env python3
"""Prove ownership of the disposable image before an ordinary mountpoint detach."""

import plistlib
import re
import sys
from pathlib import Path

DEVICE = re.compile(r"/dev/disk[0-9]+(?:s[0-9]+)*\Z")
DETACHED = "detached"
MOUNTED = "mounted"
PENDING = "pending"


def entities(value):
    if not isinstance(value, dict):
        raise ValueError("image state must be a dictionary")
    result = value.get("system-entities")
    if not isinstance(result, list) or any(not isinstance(item, dict) for item in result):
        raise ValueError("image entities are unavailable")
    return result


def mount_device(text, mountpoint):
    suffix = f" on {mountpoint} ("
    matches = [line.split(suffix, 1)[0] for line in text.splitlines() if suffix in line]
    if len(matches) > 1:
        raise ValueError("disposable mountpoint is ambiguous")
    if matches and not DEVICE.fullmatch(matches[0]):
        raise ValueError("disposable mount has an unexpected device")
    return matches[0] if matches else None


def image_device(image, mountpoint):
    matches = [item.get("dev-entry") for item in entities(image) if item.get("mount-point") == mountpoint]
    if len(matches) != 1 or not isinstance(matches[0], str) or not DEVICE.fullmatch(matches[0]):
        raise ValueError("image does not identify one exact mounted device")
    return matches[0]


def classify(attach, info, mounts, image, mountpoint):
    if not isinstance(info, dict) or not isinstance(info.get("images"), list):
        raise ValueError("live image inventory is unavailable")
    if any(not isinstance(item, dict) or not isinstance(item.get("image-path"), str) or not item["image-path"] for item in info["images"]):
        raise ValueError("live image inventory is malformed")
    exact = [item for item in info["images"] if item.get("image-path") == image]
    if len(exact) > 1:
        raise ValueError("disposable image is attached more than once")
    current = mount_device(mounts, mountpoint)
    if not exact:
        if current is not None:
            if image_device(attach, mountpoint) != current:
                raise ValueError("mountpoint no longer names the recorded disposable device")
            # Inventories are read separately; an old mount snapshot may outlive image
            # removal. Wait without issuing any detach against unknown current ownership.
            return PENDING
        return DETACHED
    if current is None:
        # Do not guess a device to detach while Disk Arbitration finishes. Wait and inspect.
        return PENDING
    recorded = image_device(attach, mountpoint)
    live = image_device(exact[0], mountpoint)
    if current != recorded or live != recorded:
        raise ValueError("disposable image, recorded device and live mount disagree")
    return MOUNTED


def main(arguments):
    if len(arguments) != 5:
        raise ValueError("expected attach plist, live plist, mounts, image and mountpoint")
    attached, live, mounts, image, mountpoint = arguments
    with Path(live).open("rb") as source:
        info = plistlib.load(source)
    # Attachment output is unnecessary to prove both mount and image absence. If still
    # mounted, malformed or missing attachment evidence must refuse detach.
    try:
        with Path(attached).open("rb") as source:
            attach = plistlib.load(source)
    except (OSError, plistlib.InvalidFileException):
        attach = None
    state = classify(attach, info, Path(mounts).read_text(), image, mountpoint)
    print(state)


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (OSError, ValueError, plistlib.InvalidFileException) as error:
        print(f"Disposable APFS ownership inspection failed: {error}", file=sys.stderr)
        sys.exit(1)
