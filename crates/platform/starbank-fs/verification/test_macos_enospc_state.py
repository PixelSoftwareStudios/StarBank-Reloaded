"""Synthetic ownership tests; never mount, detach or inspect a host device."""
import copy
import importlib.util
import unittest
import subprocess
from pathlib import Path

spec = importlib.util.spec_from_file_location("image_state", Path(__file__).with_name("macos-enospc-state.py"))
state = importlib.util.module_from_spec(spec)
spec.loader.exec_module(state)
IMAGE = "/private/tmp/starbank-apfs-enospc.synthetic/exhaustion.sparseimage"
MOUNT = "/private/tmp/starbank-apfs-enospc.synthetic/volume"
DEVICE = "/dev/disk99s1"
ATTACH = {"system-entities": [{"dev-entry": DEVICE, "mount-point": MOUNT}]}
INFO = {"images": [{"image-path": IMAGE, **ATTACH}]}
MOUNTS = f"{DEVICE} on {MOUNT} (apfs, local)\n"

class OwnershipTests(unittest.TestCase):
    def classify(self, attach=ATTACH, info=INFO, mounts=MOUNTS):
        return state.classify(attach, info, mounts, IMAGE, MOUNT)

    def test_exact_image_recorded_device_and_live_mount_match(self):
        self.assertEqual(self.classify(), state.MOUNTED)

    def test_both_absent_proves_detachment_without_attachment_output(self):
        self.assertEqual(self.classify(attach=None, info={"images": []}, mounts=""), state.DETACHED)

    def test_unmounted_attachment_only_waits(self):
        self.assertEqual(self.classify(mounts=""), state.PENDING)

    def test_image_absence_with_old_owned_mount_snapshot_only_waits(self):
        self.assertEqual(self.classify(info={"images": []}), state.PENDING)

    def test_wrong_current_device_refuses_detach(self):
        with self.assertRaises(ValueError):
            self.classify(mounts=MOUNTS.replace(DEVICE, "/dev/disk88s1"))

    def test_wrong_live_image_device_refuses_detach(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"][0]["dev-entry"] = "/dev/disk88s1"
        with self.assertRaises(ValueError):
            self.classify(info=info)

    def test_wrong_recorded_device_refuses_detach(self):
        attach = copy.deepcopy(ATTACH)
        attach["system-entities"][0]["dev-entry"] = "/dev/disk88s1"
        with self.assertRaises(ValueError):
            self.classify(attach=attach)

    def test_image_path_prefix_does_not_authorize_detach(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["image-path"] += ".unrelated"
        self.assertEqual(self.classify(info=info), state.PENDING)

    def test_duplicate_exact_images_refuse_detach(self):
        with self.assertRaises(ValueError):
            self.classify(info={"images": INFO["images"] * 2})

    def test_duplicate_mounts_refuse_detach(self):
        with self.assertRaises(ValueError):
            self.classify(mounts=MOUNTS * 2)

    def test_missing_attachment_evidence_refuses_mounted_detach(self):
        with self.assertRaises(ValueError):
            self.classify(attach=None)

    def test_missing_live_image_entities_refuse_mounted_detach(self):
        with self.assertRaises(ValueError):
            self.classify(info={"images": [{"image-path": IMAGE}]})

    def test_unrelated_mount_is_not_the_disposable_mount(self):
        self.assertEqual(self.classify(info={"images": []}, mounts=MOUNTS.replace(MOUNT, MOUNT + "-other")), state.DETACHED)

    def test_unavailable_inventory_is_not_detached_proof(self):
        for info in [{}, {"images": None}, {"images": [None]}, {"images": [{}]}, {"images": [{"image-path": ""}]}]:
            with self.subTest(info=info), self.assertRaises(ValueError):
                self.classify(info=info, mounts="")

class DetachLoopTests(unittest.TestCase):
    def run_loop(self, states, detach_success=False):
        script = Path(__file__).with_name("macos-enospc.sh").read_text()
        start = script.index("detach_owned_image() {")
        loop = script[start:script.index("\ncleanup() {", start)]
        loop = loop.replace("/usr/bin/hdiutil detach", "fake_detach").replace("/bin/sleep", "fake_sleep")
        cases = "\n".join(f"{index}) fixture_state={value} ;;" if value != "error" else f"{index}) return 1 ;;" for index, value in enumerate(states, 1))
        body = f"""
max_detach_attempts=6
detach_retry_seconds=1
mountpoint=/synthetic/owned/volume
checks=0
detaches=0
sleeps=0
inspect_state() {{ checks=$((checks + 1)); case "$checks" in
{cases}
*) fixture_state={states[-1] if states[-1] != 'error' else 'pending'} ;;
esac
}}
fake_detach() {{ [ "$#" -eq 1 ] && [ "$1" = "$mountpoint" ] || exit 90; detaches=$((detaches + 1)); return {0 if detach_success else 1}; }}
fake_sleep() {{ sleeps=$((sleeps + 1)); }}
{loop}
if detach_owned_image; then result=0; else result=$?; fi
printf '%s %s %s %s' "$result" "$checks" "$detaches" "$sleeps"
"""
        result = subprocess.run(["/bin/sh"], input=body, text=True, capture_output=True, check=True)
        return tuple(map(int, result.stdout.split()))

    def test_busy_then_disappearance_retries_owned_mount(self):
        self.assertEqual(self.run_loop(["mounted", "mounted", "mounted", "detached"]), (0, 4, 2, 1))

    def test_permanent_busy_has_bounded_attempts_and_failure(self):
        self.assertEqual(self.run_loop(["mounted"]), (1, 12, 6, 5))

    def test_unmounted_attachment_waits_without_detach(self):
        self.assertEqual(self.run_loop(["pending", "pending", "detached"]), (0, 3, 0, 1))

    def test_inspection_failure_never_detaches(self):
        self.assertEqual(self.run_loop(["error"]), (1, 1, 0, 0))

    def test_changed_ownership_before_retry_prevents_another_detach(self):
        self.assertEqual(self.run_loop(["mounted", "mounted", "error"]), (1, 3, 1, 1))

    def test_command_success_without_absence_proof_still_fails(self):
        self.assertEqual(self.run_loop(["mounted"], detach_success=True), (1, 12, 6, 5))

if __name__ == "__main__":
    unittest.main()
