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
ROOT = "/dev/disk98"
DEVICE = "/dev/disk99s1"
ATTACH = {"system-entities": [{"dev-entry": ROOT, "content-hint": "GUID_partition_scheme"}, {"dev-entry": DEVICE, "mount-point": MOUNT}]}
INFO = {"images": [{"image-path": IMAGE, **ATTACH}]}
MOUNTS = f"{DEVICE} on {MOUNT} (apfs, local)\n"

class OwnershipTests(unittest.TestCase):
    def classify(self, attach=ATTACH, info=INFO, mounts=MOUNTS):
        return state.classify(attach, info, mounts, IMAGE, MOUNT)

    def test_exact_image_recorded_device_and_live_mount_match(self):
        self.assertEqual(self.classify(), state.MOUNTED)

    def test_both_absent_proves_detachment_without_attachment_output(self):
        self.assertEqual(self.classify(attach=None, info={"images": []}, mounts=""), state.DETACHED)

    def test_unmounted_exact_image_authorizes_original_root_detach(self):
        self.assertEqual(self.classify(mounts=""), state.ATTACHED_UNMOUNTED + ROOT)

    def test_image_absence_with_old_owned_mount_snapshot_only_waits(self):
        self.assertEqual(self.classify(info={"images": []}), state.IMAGE_ABSENT_STALE_MOUNT)

    def test_wrong_current_device_refuses_detach(self):
        with self.assertRaises(ValueError):
            self.classify(mounts=MOUNTS.replace(DEVICE, "/dev/disk88s1"))

    def test_wrong_live_image_device_refuses_detach(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"][1]["dev-entry"] = "/dev/disk88s1"
        with self.assertRaises(ValueError):
            self.classify(info=info)

    def test_wrong_recorded_device_refuses_detach(self):
        attach = copy.deepcopy(ATTACH)
        attach["system-entities"][1]["dev-entry"] = "/dev/disk88s1"
        with self.assertRaises(ValueError):
            self.classify(attach=attach)

    def test_image_path_prefix_does_not_authorize_detach(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["image-path"] += ".unrelated"
        with self.assertRaises(ValueError):
            self.classify(info=info)

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
        self.assertEqual(self.classify(info={"images": []}, mounts=MOUNTS.replace(MOUNT, MOUNT + "-other").replace(DEVICE, "/dev/disk88s1")), state.DETACHED)

    def test_unavailable_inventory_is_not_detached_proof(self):
        for info in [{}, {"images": None}, {"images": [None]}, {"images": [{}]}, {"images": [{"image-path": ""}]}]:
            with self.subTest(info=info), self.assertRaises(ValueError):
                self.classify(info=info, mounts="")

    def test_missing_live_leaf_with_exact_root_authorizes_device_detach(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"].pop()
        self.assertEqual(self.classify(info=info, mounts=""), state.ATTACHED_UNMOUNTED + ROOT)

    def test_stale_mount_with_disappeared_leaf_only_waits(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"].pop()
        self.assertEqual(self.classify(info=info), state.ATTACHED_STALE_MOUNT)

    def test_stale_mount_with_dropped_live_binding_only_waits(self):
        info = copy.deepcopy(INFO)
        del info["images"][0]["system-entities"][1]["mount-point"]
        self.assertEqual(self.classify(info=info), state.ATTACHED_STALE_MOUNT)

    def test_foreign_mount_on_recorded_leaf_refuses_even_if_image_absent(self):
        for info in [INFO, {"images": []}]:
            with self.subTest(info=info), self.assertRaises(ValueError):
                self.classify(info=info, mounts=MOUNTS.replace(MOUNT, MOUNT + "-foreign"))

    def test_foreign_mount_on_original_root_refuses(self):
        with self.assertRaises(ValueError):
            self.classify(mounts=f"{ROOT} on /foreign (hfs, local)\n")

    def test_foreign_live_image_mountpoint_refuses_without_mount_table_binding(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"][1]["mount-point"] = MOUNT + "-foreign"
        with self.assertRaises(ValueError):
            self.classify(info=info, mounts="")

    def test_changed_root_refuses_device_detach(self):
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"][0]["dev-entry"] = "/dev/disk88"
        with self.assertRaises(ValueError):
            self.classify(info=info, mounts="")

    def test_missing_unknown_or_duplicate_gpt_roots_refuse(self):
        for root in [None, {"dev-entry": ROOT, "content-hint": "unknown"}, {"dev-entry": DEVICE, "content-hint": "GUID_partition_scheme"}]:
            info = copy.deepcopy(INFO)
            info["images"][0]["system-entities"] = [info["images"][0]["system-entities"][1]]
            if root:
                info["images"][0]["system-entities"].append(root)
            with self.subTest(root=root), self.assertRaises(ValueError):
                self.classify(info=info, mounts="")
        info = copy.deepcopy(INFO)
        info["images"][0]["system-entities"].append({"dev-entry": "/dev/disk88", "content-hint": "GUID_partition_scheme"})
        with self.assertRaises(ValueError):
            self.classify(info=info, mounts="")

    def test_foreign_image_claiming_any_recorded_device_refuses(self):
        for device in [ROOT, DEVICE]:
            info = copy.deepcopy(INFO)
            info["images"].append({"image-path": IMAGE + ".foreign", "system-entities": [{"dev-entry": device}]})
            with self.subTest(device=device), self.assertRaises(ValueError):
                self.classify(info=info, mounts="")

    def test_absent_exact_path_with_foreign_root_claim_refuses_absence(self):
        info = {"images": [{"image-path": IMAGE + ".alias", "system-entities": [{"dev-entry": ROOT}]}]}
        with self.assertRaises(ValueError):
            self.classify(info=info, mounts="")

    def test_absent_exact_path_with_foreign_leaf_claim_refuses_absence(self):
        info = {"images": [{"image-path": IMAGE + ".alias", "system-entities": [{"dev-entry": DEVICE}]}]}
        with self.assertRaises(ValueError):
            self.classify(info=info, mounts="")

    def test_absent_exact_path_with_foreign_claim_refuses_stale_wait(self):
        info = {"images": [{"image-path": IMAGE + ".alias", "system-entities": [{"dev-entry": ROOT}]}]}
        with self.assertRaises(ValueError):
            self.classify(info=info)

    def test_malformed_or_duplicate_live_devices_refuse(self):
        for entity in [{}, {"dev-entry": ROOT}, {"dev-entry": "/dev/disk88;unsafe"}]:
            info = copy.deepcopy(INFO)
            info["images"][0]["system-entities"].append(entity)
            with self.subTest(entity=entity), self.assertRaises(ValueError):
                self.classify(info=info, mounts="")

    def test_unknown_mount_source_at_exact_mountpoint_refuses(self):
        with self.assertRaises(ValueError):
            self.classify(info={"images": []}, mounts=f"unknown on {MOUNT} (apfs, local)\n")

    def test_malformed_mount_inventory_never_proves_absence(self):
        for mounts in ["unparseable", f"{DEVICE} on {MOUNT}", f"{DEVICE} on {MOUNT} (apfs"]:
            with self.subTest(mounts=mounts), self.assertRaises(ValueError):
                self.classify(info={"images": []}, mounts=mounts)

class DetachLoopTests(unittest.TestCase):
    def run_loop(self, states, detach_success=False):
        script = Path(__file__).with_name("macos-enospc.sh").read_text()
        start = script.index("detach_owned_image() {")
        loop = script[start:script.index("\ncleanup() {", start)]
        loop = loop.replace("/usr/bin/hdiutil detach", "fake_detach").replace("/bin/sleep", "fake_sleep")
        cases = "\n".join(f"{index}) fixture_state={value} ;;" if value != "error" else f"{index}) return 1 ;;" for index, value in enumerate(states, 1))
        body = f"""
max_detach_attempts=12
detach_retry_seconds=2
mountpoint=/synthetic/owned/volume
checks=0
detaches=0
sleeps=0
device_detaches=0
inspect_state() {{ checks=$((checks + 1)); case "$checks" in
{cases}
*) fixture_state={states[-1] if states[-1] != 'error' else 'attached_stale_mount'} ;;
esac
}}
fake_detach() {{ [ "$#" -eq 1 ] || exit 90; case "$1" in "$mountpoint") ;; /dev/disk98) device_detaches=$((device_detaches + 1)) ;; *) exit 91 ;; esac; detaches=$((detaches + 1)); return {0 if detach_success else 1}; }}
fake_sleep() {{ sleeps=$((sleeps + 1)); }}
{loop}
if detach_owned_image; then result=0; else result=$?; fi
printf '%s %s %s %s %s' "$result" "$checks" "$detaches" "$sleeps" "$device_detaches"
"""
        result = subprocess.run(["/bin/sh"], input=body, text=True, capture_output=True, check=True)
        return tuple(map(int, result.stdout.split()))

    def test_actual_retry_settling_budget_does_not_exceed_thirty_seconds(self):
        script = Path(__file__).with_name("macos-enospc.sh").read_text()
        assignments = dict(line.split("=", 1) for line in script.splitlines() if line.startswith(("max_detach_attempts=", "detach_retry_seconds=")))
        attempts = int(assignments["max_detach_attempts"])
        delay = int(assignments["detach_retry_seconds"])
        self.assertGreater(attempts, 0)
        self.assertGreater(delay, 0)
        self.assertLessEqual((attempts - 1) * delay, 30)

    def test_busy_then_disappearance_retries_owned_mount(self):
        self.assertEqual(self.run_loop(["mounted", "mounted", "mounted", "detached"]), (0, 4, 2, 1, 0))

    def test_permanent_busy_has_bounded_attempts_and_failure(self):
        self.assertEqual(self.run_loop(["mounted"]), (1, 24, 12, 11, 0))

    def test_image_absent_stale_mount_waits_without_detach(self):
        self.assertEqual(self.run_loop(["image_absent_stale_mount", "image_absent_stale_mount", "detached"]), (0, 3, 0, 1, 0))

    def test_inspection_failure_never_detaches(self):
        self.assertEqual(self.run_loop(["error"]), (1, 1, 0, 0, 0))

    def test_changed_ownership_before_retry_prevents_another_detach(self):
        self.assertEqual(self.run_loop(["mounted", "mounted", "error"]), (1, 3, 1, 1, 0))

    def test_command_success_without_absence_proof_still_fails(self):
        self.assertEqual(self.run_loop(["mounted"], detach_success=True), (1, 24, 12, 11, 0))

    def test_busy_mount_then_owned_unmounted_root_retries_device(self):
        self.assertEqual(self.run_loop(["mounted", "attached_unmounted:/dev/disk98", "attached_unmounted:/dev/disk98", "detached"]), (0, 4, 2, 1, 1))

    def test_permanently_owned_unmounted_root_is_bounded_failure(self):
        self.assertEqual(self.run_loop(["attached_unmounted:/dev/disk98"]), (1, 24, 12, 11, 12))

    def test_attached_stale_mount_only_waits(self):
        self.assertEqual(self.run_loop(["attached_stale_mount", "attached_stale_mount", "detached"]), (0, 3, 0, 1, 0))

    def test_ownership_failure_prevents_device_retry(self):
        self.assertEqual(self.run_loop(["attached_unmounted:/dev/disk98", "attached_unmounted:/dev/disk98", "error"]), (1, 3, 1, 1, 1))

if __name__ == "__main__":
    unittest.main()
