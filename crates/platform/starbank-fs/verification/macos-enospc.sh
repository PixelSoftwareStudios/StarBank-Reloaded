#!/bin/sh
set -eu
export LC_ALL=C

cd "$(dirname "$0")/../../../.."
if [ "$(uname -s)" != Darwin ]; then
	printf '%s\n' 'The disposable APFS qualification requires macOS.' >&2
	exit 1
fi

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
inspect_state() {
	mounts=$(/sbin/mount) || return 1
	images=$(/usr/bin/hdiutil info) || return 1
}

owns_mount() {
	printf '%s\n' "$mounts" | /usr/bin/grep -Fq -- " on $mountpoint ("
}

image_attached() {
	printf '%s\n' "$images" | /usr/bin/grep -Fq -- "$image"
}

cleanup() {
	status=$?
	trap - EXIT
	if ! inspect_state; then
		printf 'Unable to inspect mounts; retaining disposable fixture at %s\n' "$scratch" >&2
		exit 1
	fi
	if owns_mount; then
		if ! image_attached || ! /usr/bin/hdiutil detach "$mountpoint"; then
			printf 'Unable to detach disposable image at %s\n' "$mountpoint" >&2
			exit 1
		fi
	fi
	if ! inspect_state; then
		printf 'Unable to verify detachment; retaining disposable fixture at %s\n' "$scratch" >&2
		exit 1
	fi
	if image_attached; then
		printf 'Disposable image remains attached at %s\n' "$image" >&2
		exit 1
	fi
	if [ "$status" -ne 0 ]; then
		printf 'Retaining failed qualification evidence at %s\n' "$scratch" >&2
		exit "$status"
	fi
	rm -f -- "$image" "$attached"
	rmdir -- "$mountpoint" "$scratch"
	exit "$status"
}

mkdir "$mountpoint"
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
/usr/bin/hdiutil create -size 128m -type SPARSE -fs APFS -volname StarBankENOSPC "$image"
/usr/bin/hdiutil attach -nobrowse -noautoopen -owners on -mountpoint "$mountpoint" -plist "$image" >"$attached"
if ! inspect_state || ! owns_mount || ! image_attached; then
	printf 'The disposable image did not mount at %s\n' "$mountpoint" >&2
	exit 1
fi

STARBANK_ENOSPC_APFS="$mountpoint" cargo nextest run --locked --offline -p starbank-fs \
	--test macos_enospc --run-ignored only --no-tests fail --success-output immediate
