#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    printf 'Usage: sh %s /path/to/tla2tools.jar\n' "$0" >&2
    exit 2
fi

jar_dir=$(CDPATH='' cd -- "$(dirname -- "$1")" && pwd)
jar="$jar_dir/$(basename -- "$1")"
model_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
expected_sha=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
LC_ALL=C
export LC_ALL
if ! printf '%s  %s\n' "$expected_sha" "$jar" | shasum -a 256 -c >/dev/null; then
    printf 'Expected TLA+ Tools v1.7.4 with SHA-256 %s.\n' "$expected_sha" >&2
    exit 1
fi

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/starbank-save-backup-model.XXXXXXXX")
cp "$model_dir/SaveBackup.tla" "$model_dir/SaveBackup.cfg" "$run_dir/"
cd "$run_dir"
printf 'TLC logs and state files are in %s\n' "$run_dir"

check() {
    run_name=$1
    java -XX:+UseParallelGC -Xmx512m -Djava.rmi.server.hostname=127.0.0.1 \
        -jar "$jar" -workers 1 -seed 1 -fp 0 -coverage 1 \
        -metadir "$run_name-states" -config "$run_name.cfg" SaveBackup.tla \
        > "$run_name.log" 2>&1
}

check SaveBackup || { cat SaveBackup.log >&2; exit 1; }
if ! grep -F 'Model checking completed. No error has been found.' SaveBackup.log; then
    cat SaveBackup.log >&2
    exit 1
fi
grep -E '^([[:digit:]]+ states generated,|The depth of the complete state graph search)' SaveBackup.log

counterexample() {
    fault=$1
    invariant=$2
    sed "s/^CONSTANT Fault = \"None\"$/CONSTANT Fault = \"$fault\"/" \
        SaveBackup.cfg > "$fault.cfg"
    status=0
    check "$fault" || status=$?
    if [ "$status" -ne 12 ] || ! grep -Fq "Invariant $invariant is violated." "$fault.log"; then
        cat "$fault.log" >&2
        exit 1
    fi
    printf 'Expected counterexample found for %s (%s)\n' "$fault" "$invariant"
}

counterexample SkipBackup ProtectedExchangeHasRecordedBackup
counterexample IgnoreDisplacedConflict VerifiedAfterSyncAndBothObservations
counterexample BlindRetry RetryRequiresReopen
