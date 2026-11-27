#!/usr/bin/env python3
"""Prove exact disposable-image ownership before ordinary mountpoint/device detach."""

import plistlib
import re
import sys
from pathlib import Path

DEVICE = re.compile(r"/dev/disk[0-9]+(?:s[0-9]+)*\Z")
ROOT_DEVICE = re.compile(r"/dev/disk[0-9]+\Z")
GPT_HINT = "GUID_partition_scheme"
DETACHED = "detached"
MOUNTED = "mounted"
IMAGE_ABSENT_STALE_MOUNT = "image_absent_stale_mount"
ATTACHED_STALE_MOUNT = "attached_stale_mount"
ATTACHED_UNMOUNTED = "attached_unmounted:"


def entities(value):
    if not isinstance(value, dict):
        raise ValueError("image state must be a dictionary")
    result = value.get("system-entities")
    if not isinstance(result, list) or any(not isinstance(item, dict) for item in result):
        raise ValueError("image entities are unavailable")
    devices = [item.get("dev-entry") for item in result]
    if any(not isinstance(device, str) or not DEVICE.fullmatch(device) for device in devices):
        raise ValueError("image entity device is unavailable")
    if len(set(devices)) != len(devices):
        raise ValueError("image entity devices are ambiguous")
    return result


def mounted_devices(text):
    result = []
    for line in text.splitlines():
        if not line:
            continue
        device, separator, remainder = line.partition(" on ")
        if not separator or not device:
            raise ValueError("device mount entry is malformed")
        mountpoint, separator, flags = remainder.rpartition(" (")
        if not separator or not mountpoint or not flags.endswith(")"):
            raise ValueError("device mount entry is malformed")
        result.append((device, mountpoint))
    return result


def mount_device(mounts, mountpoint):
    matches = [device for device, path in mounts if path == mountpoint]
    if len(matches) > 1:
        raise ValueError("disposable mountpoint is ambiguous")
    if matches and not DEVICE.fullmatch(matches[0]):
        raise ValueError("disposable mount has an unexpected device")
    return matches[0] if matches else None


def image_device(image, mountpoint):
    matches = [item["dev-entry"] for item in entities(image) if item.get("mount-point") == mountpoint]
    if len(matches) != 1:
        raise ValueError("image does not identify one exact mounted device")
    return matches[0]


def image_root(image):
    # GPTSPUD is explicit at creation. Unknown plist hints/layouts refuse authority;
    # never infer a physical root from a synthesized APFS volume's disk number.
    matches = [item["dev-entry"] for item in entities(image) if item.get("content-hint") == GPT_HINT]
    if len(matches) != 1 or not ROOT_DEVICE.fullmatch(matches[0]):
        raise ValueError("image does not identify one unique whole GPT device")
    return matches[0]


def refuse_foreign_mounts(mounts, related, mountpoint):
    if any(device in related and path != mountpoint for device, path in mounts):
        raise ValueError("a disposable-image device is mounted elsewhere")


def refuse_image_claims(images, related, owned=None):
    for other in images:
        if other is not owned and any(item["dev-entry"] in related for item in entities(other)):
            raise ValueError("a disposable-image device is claimed by another image")


def classify(attach, info, mounts, image, mountpoint):
    if not isinstance(info, dict) or not isinstance(info.get("images"), list):
        raise ValueError("live image inventory is unavailable")
    if any(not isinstance(item, dict) or not isinstance(item.get("image-path"), str) or not item["image-path"] for item in info["images"]):
        raise ValueError("live image inventory is malformed")
    exact = [item for item in info["images"] if item["image-path"] == image]
    if len(exact) > 1:
        raise ValueError("disposable image is attached more than once")
    mounts = mounted_devices(mounts)
    current = mount_device(mounts, mountpoint)
    if not exact:
        if attach is not None:
            related = {item["dev-entry"] for item in entities(attach)}
            refuse_image_claims(info["images"], related)
            refuse_foreign_mounts(mounts, related, mountpoint)
        if current is not None:
            if image_device(attach, mountpoint) != current:
                raise ValueError("mountpoint no longer names the recorded disposable device")
            return IMAGE_ABSENT_STALE_MOUNT
        return DETACHED

    recorded = image_device(attach, mountpoint)
    root = image_root(attach)
    if image_root(exact[0]) != root:
        raise ValueError("live image root no longer matches the original GPT device")
    live = entities(exact[0])
    related = {item["dev-entry"] for item in entities(attach) + live}
    refuse_image_claims(info["images"], related, exact[0])
    refuse_foreign_mounts(mounts, related, mountpoint)
    if any(item.get("mount-point") not in (None, "", mountpoint) for item in live):
        raise ValueError("live disposable image records a foreign mountpoint")
    bindings = [item["dev-entry"] for item in live if item.get("mount-point") == mountpoint]
    if bindings and bindings != [recorded]:
        raise ValueError("live disposable mount binding disagrees with attachment")
    if current is None:
        # Partial APFS teardown may drop the synthesized leaf first. The unique
        # original GPT root must still belong to this exact live image path.
        return ATTACHED_UNMOUNTED + root
    if current != recorded:
        raise ValueError("disposable mount no longer names the recorded device")
    if not bindings:
        # Separate inventory reads may observe an old mount after its live image
        # binding disappeared. Wait; this is not device-detach authority.
        return ATTACHED_STALE_MOUNT
    return MOUNTED


def main(arguments):
    if len(arguments) != 5:
        raise ValueError("expected attach plist, live plist, mounts, image and mountpoint")
    attached, live, mounts, image, mountpoint = arguments
    with Path(live).open("rb") as source:
        info = plistlib.load(source)
    try:
        with Path(attached).open("rb") as source:
            attach = plistlib.load(source)
    except (OSError, plistlib.InvalidFileException):
        attach = None
    print(classify(attach, info, Path(mounts).read_text(), image, mountpoint))


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (OSError, ValueError, plistlib.InvalidFileException) as error:
        print(f"Disposable APFS ownership inspection failed: {error}", file=sys.stderr)
        sys.exit(1)
