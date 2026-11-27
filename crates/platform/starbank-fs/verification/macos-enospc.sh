#!/bin/sh
set -eu
export LC_ALL=C

cd "$(dirname "$0")/../../../.."
if [ "$(uname -s)" != Darwin ]; then
	printf '%s\n' 'The disposable APFS qualification requires macOS.' >&2
	exit 1
fi

python3 -B -m unittest discover -s crates/platform/starbank-fs/verification -p 'test_macos_enospc_state.py'

host_free_kib=$(/bin/df -Pk /private/tmp | /usr/bin/awk 'NR == 2 { print $4 }')
case "$host_free_kib" in
	''|*[!0-9]*)
		printf '%s\n' 'Unable to verify free host space under /private/tmp.' >&2
		exit 1
		;;
esac
if [ "$host_free_kib" -lt 1048576 ]; then
	printf '%s\n' 'The host must have at least 1 GiB free under /private/tmp.' >&2
	exit 1
fi

scratch=$(mktemp -d /private/tmp/starbank-apfs-enospc.XXXXXXXX)
mountpoint="$scratch/volume"
image="$scratch/exhaustion.sparseimage"
attached="$scratch/attach.plist"
image_state="$scratch/image-state.plist"
mount_state="$scratch/mount-state.txt"
state_checker=crates/platform/starbank-fs/verification/macos-enospc-state.py
max_detach_attempts=12
detach_retry_seconds=2
inspect_state() {
	/sbin/mount >"$mount_state" || return 1
	/usr/bin/hdiutil info -plist >"$image_state" || return 1
	fixture_state=$(python3 -B "$state_checker" "$attached" "$image_state" "$mount_state" "$image" "$mountpoint") || return 1
}

detach_owned_image() {
	detach_attempt=1
	while [ "$detach_attempt" -le "$max_detach_attempts" ]; do
		if ! inspect_state; then
			printf '%s\n' 'Unable to establish disposable image ownership; refusing detach.' >&2
			return 1
		fi
		case "$fixture_state" in
			detached) return 0 ;;
			mounted)
				printf 'Ordinary disposable image detach attempt %s/%s: mounted\n' "$detach_attempt" "$max_detach_attempts" >&2
				if ! /usr/bin/hdiutil detach "$mountpoint"; then
					printf '%s\n' 'Ordinary detach failed; rechecking the exact owned image before retry.' >&2
				fi
				;;
			# hdiutil(1) supports an explicit /dev node; the checker proves ownership.
			attached_unmounted:*)
				detach_device=${fixture_state#attached_unmounted:}
				printf 'Ordinary disposable image root detach attempt %s/%s: attached_unmounted %s\n' "$detach_attempt" "$max_detach_attempts" "$detach_device" >&2
				if ! /usr/bin/hdiutil detach "$detach_device"; then
					printf '%s\n' 'Ordinary device detach failed; rechecking exact image ownership.' >&2
				fi
				;;
			image_absent_stale_mount|attached_stale_mount)
				printf 'Disposable image transition %s at attempt %s/%s; waiting for fresh evidence.\n' "$fixture_state" "$detach_attempt" "$max_detach_attempts" >&2
				;;
			*) return 1 ;;
		esac
		if ! inspect_state; then return 1; fi
		if [ "$fixture_state" = detached ]; then return 0; fi
		if [ "$detach_attempt" -lt "$max_detach_attempts" ]; then
			/bin/sleep "$detach_retry_seconds"
		fi
		detach_attempt=$((detach_attempt + 1))
	done
	printf '%s\n' 'Disposable image remains attached after bounded ordinary detach attempts.' >&2
	return 1
}

cleanup() {
	status=$?
	trap - EXIT
	if ! detach_owned_image; then
		printf 'Unable to detach disposable image; retaining evidence at %s\n' "$scratch" >&2
		exit 1
	fi
	if ! inspect_state || [ "$fixture_state" != detached ]; then
		printf 'Unable to prove mount and image absence; retaining fixture at %s\n' "$scratch" >&2
		exit 1
	fi
	if [ "$status" -ne 0 ]; then
		printf 'Retaining failed qualification evidence at %s\n' "$scratch" >&2
		exit "$status"
	fi
	rm -f -- "$image" "$attached" "$image_state" "$mount_state"
	rmdir -- "$mountpoint" "$scratch"
	exit "$status"
}

mkdir "$mountpoint"
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
/usr/bin/hdiutil create -size 128m -type SPARSE -layout GPTSPUD -fs APFS -volname StarBankENOSPC "$image"
/usr/bin/hdiutil attach -nobrowse -noautoopen -owners on -mountpoint "$mountpoint" -plist "$image" >"$attached"
if ! inspect_state || [ "$fixture_state" != mounted ]; then
	printf 'The disposable image did not mount at %s\n' "$mountpoint" >&2
	exit 1
fi

STARBANK_ENOSPC_APFS="$mountpoint" cargo nextest run --locked --offline -p starbank-fs \
	--test macos_enospc --run-ignored only --no-tests fail --success-output immediate
