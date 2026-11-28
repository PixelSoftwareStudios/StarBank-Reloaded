#!/usr/bin/env bash
set -euo pipefail

# Ubuntu 24.04 (noble-20261110.1), with native amd64 and arm64 manifests.
image=docker.io/library/ubuntu:24.04@sha256:562111b88caa79c71391c0c4d3c2114a7d5c35915c20561fd893cb1e07929e6b

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

if [[ $# -ne 3 ]]; then
  printf 'Usage: %s <verified-deb-path> <version> <amd64|arm64>\n' "${0##*/}" >&2
  exit 2
fi
input=$1
version=$2
architecture=$3
case $architecture in
  amd64 | arm64) ;;
  *) fail 'Expected Debian architecture amd64 or arm64.' ;;
esac
[[ $version =~ ^[0-9][0-9A-Za-z.+~:-]*$ ]] || fail 'Invalid Debian version.'
filename="starbank-reloaded_${version}_${architecture}.deb"
[[ ${input##*/} == "$filename" ]] || fail "Expected the exact package filename $filename."
[[ -f $input && ! -L $input && -s $input ]] || fail 'Expected a nonempty regular DEB file.'
directory=$(cd -P -- "$(dirname -- "$input")" && printf '%s/' "${PWD%/}")
package="$directory$filename"
case "$input$package" in
  *','* | *'"'* | *$'\r'* | *$'\n'*) fail 'The package path cannot form a Docker bind mount.' ;;
esac

if command -v sha256sum >/dev/null; then
  digest=$(sha256sum < "$package")
elif command -v shasum >/dev/null; then
  digest=$(shasum -a 256 < "$package")
else
  fail 'Expected sha256sum or shasum to hash the DEB.'
fi
package_hash=${digest%% *}
[[ $package_hash =~ ^[0-9a-f]{64}$ ]] || fail 'Could not read a SHA-256 digest.'
daemon_arch=$(docker info --format '{{.Architecture}}')
case "$architecture:$daemon_arch" in
  amd64:amd64 | amd64:x86_64 | arm64:arm64 | arm64:aarch64) ;;
  *) fail "Docker must run the package architecture natively (daemon: $daemon_arch)." ;;
esac
docker version --format 'Docker client {{.Client.Version}}; server {{.Server.Version}}'
printf '%s %s\n' "$package_hash" "$filename"
printf 'Ubuntu image: %s\n' "$image"

container_id=
cleanup() {
  local status=$?
  trap - EXIT
  if [[ $container_id =~ ^[0-9a-f]{64}$ ]] && ! docker rm --force "$container_id" >/dev/null; then
    printf 'Could not remove owned verification container %s.\n' "$container_id" >&2
    if [[ $status -eq 0 ]]; then status=1; fi
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

run_container() {
  local mode=$1 network=$2
  printf 'Qualifying %s in a fresh %s container.\n' "$mode" "$architecture"
  container_id=$(docker create --interactive --platform "linux/$architecture" \
    --network "$network" \
    --mount "type=bind,source=$package,target=/tmp/starbank.deb,readonly" \
    --entrypoint /bin/sh "$image" -eu -s -- "$mode" "$version" "$architecture" "$package_hash")
  [[ $container_id =~ ^[0-9a-f]{64}$ ]] || fail 'Docker did not return a container ID.'
  docker start --attach --interactive "$container_id" <<'CONTAINER_SH'
export LC_ALL=C DEBIAN_FRONTEND=noninteractive
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
mode=$1
version=$2
architecture=$3
package_hash=$4
package=/tmp/starbank.deb
package_name=starbank-reloaded
binary=/usr/bin/starbank-desktop
launcher=/usr/share/applications/starbank-reloaded.desktop

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

sha256_of() {
  result=$(sha256sum < "$1")
  printf '%s\n' "${result%% *}"
}

assert_no_package_records() {
  records=$(dpkg-query -W -f='${Package}\n')
  for unwanted do
    if printf '%s\n' "$records" | grep -Fqx -- "$unwanted"; then
      fail "Unexpected package record for $unwanted."
    fi
  done
}

assert_no_build_or_display_tools() {
  assert_no_package_records libgtk-3-dev libwebkit2gtk-4.1-dev libx11-dev libxdo-dev \
    webkit2gtk-driver xvfb xdotool
}

assert_no_payload() {
  for path in "$binary" "$launcher"; do
    [ ! -e "$path" ] && [ ! -L "$path" ] || fail "Unexpected existing payload: $path."
  done
}

assert_package_state() {
  actual=$(dpkg-query -W -f='${Status} ${Version} ${Architecture}' "$package_name")
  [ "$actual" = "install ok $1 $version $architecture" ] ||
    fail "Unexpected $package_name package state: $actual."
}

assert_payload() {
  for path in "$binary" "$launcher"; do
    [ -f "$path" ] && [ ! -L "$path" ] || fail "Expected a regular package file: $path."
    [ "$(dpkg-query -S "$path")" = "$package_name: $path" ] ||
      fail "Unexpected package file owner: $path."
  done
  [ -x "$binary" ] || fail 'The installed binary is not executable.'
  [ "$(sha256_of "$binary")" = "$binary_hash" ] || fail 'The installed binary differs from the DEB.'
  [ "$(sha256_of "$launcher")" = "$launcher_hash" ] || fail 'The installed launcher differs from the DEB.'
  grep -Fxq 'Exec=starbank-desktop' "$launcher" || fail 'Unexpected desktop launcher command.'
  [ "$(command -v starbank-desktop)" = "$binary" ] || fail 'The launcher command did not resolve.'
}

assert_clean_package_database() {
  audit=$(dpkg --audit)
  [ -z "$audit" ] || fail "$audit"
}

grep -Fxq 'ID=ubuntu' /etc/os-release || fail 'Expected an Ubuntu container.'
grep -Fxq 'VERSION_ID="24.04"' /etc/os-release || fail 'Expected Ubuntu 24.04.'
[ "$(dpkg --print-architecture)" = "$architecture" ] || fail 'Wrong container architecture.'
dpkg --validate-version "$version"
[ "$(sha256_of "$package")" = "$package_hash" ] || fail 'The mounted package differs from the verified DEB.'
[ "$(dpkg-deb --field "$package" Package)" = "$package_name" ] || fail 'Wrong mounted package name.'
[ "$(dpkg-deb --field "$package" Version)" = "$version" ] || fail 'Wrong mounted package version.'
[ "$(dpkg-deb --field "$package" Architecture)" = "$architecture" ] ||
  fail 'Wrong mounted package architecture.'
assert_clean_package_database
assert_no_package_records "$package_name" libgtk-3-0 libgtk-3-0t64 libwebkit2gtk-4.1-0
assert_no_build_or_display_tools
assert_no_payload

payload=$(mktemp -d /tmp/starbank-payload.XXXXXXXX)
dpkg-deb --raw-extract "$package" "$payload"
for hook in preinst postinst prerm postrm triggers; do
  path="$payload/DEBIAN/$hook"
  [ ! -e "$path" ] && [ ! -L "$path" ] || fail "Unexpected package installation hook: $hook."
done
for path in "$payload$binary" "$payload$launcher"; do
  [ -f "$path" ] && [ ! -L "$path" ] || fail 'Expected a regular binary and launcher in the DEB.'
done
binary_hash=$(sha256_of "$payload$binary")
launcher_hash=$(sha256_of "$payload$launcher")
printf '%s %s\n' "$binary_hash" "$binary" "$launcher_hash" "$launcher"
rm -r -- "$payload"

case $mode in
  missing-dependencies)
    dpkg --unpack "$package" </dev/null
    configure_status=0
    configure_output=$(dpkg --configure "$package_name" </dev/null 2>&1) || configure_status=$?
    printf '%s\n' "$configure_output"
    [ "$configure_status" -eq 1 ] || fail 'Expected dpkg configuration to reject missing dependencies.'
    for diagnostic in 'dependency problems - leaving unconfigured' \
      'depends on libgtk-3-0' 'depends on libwebkit2gtk-4.1-0'; do
      printf '%s\n' "$configure_output" | grep -Fq -- "$diagnostic" ||
        fail "Missing expected dpkg dependency diagnostic: $diagnostic."
    done
    assert_package_state unpacked
    assert_payload
    ;;
  apt-runtime-install)
    apt-get update </dev/null
    apt-get install --yes --no-install-recommends "$package" </dev/null
    apt-get check </dev/null
    assert_clean_package_database
    assert_package_state installed
    assert_no_build_or_display_tools
    assert_payload
    case $architecture in
      amd64) loader=/lib64/ld-linux-x86-64.so.2 ;;
      arm64) loader=/lib/ld-linux-aarch64.so.1 ;;
      *) fail 'Unexpected dynamic loader architecture.' ;;
    esac
    # Invoke the loader modes for a trusted ELF without entering the application.
    "$loader" --verify "$binary"
    libraries=$("$loader" --list "$binary")
    printf '%s\n' "$libraries"
    [ -n "$libraries" ] || fail 'The dynamic loader reported no dependencies.'
    if printf '%s\n' "$libraries" | grep -Fq 'not found'; then
      fail 'An ELF dependency did not resolve.'
    fi
    dpkg-query -W -f='${binary:Package} ${Version}\n' libc6 'libgtk-3-0*' libwebkit2gtk-4.1-0
    ;;
  *) fail 'Unknown verification mode.' ;;
esac

dpkg --purge "$package_name" </dev/null
assert_clean_package_database
assert_no_package_records "$package_name"
assert_no_payload
printf '%s: %s completed and the owned package was purged.\n' "$architecture" "$mode"
CONTAINER_SH
  docker rm "$container_id" >/dev/null
  container_id=
}

run_container missing-dependencies none
run_container apt-runtime-install bridge
