#!/bin/sh
set -eu

fuzz_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$fuzz_dir/../../../.." && pwd)
toolchain=nightly-2026-11-04
expected_cargo_fuzz='cargo-fuzz 0.13.2'
target=${1:-lossless}
case "$target" in
    lossless|structural) ;;
    *)
        printf 'Expected fuzz target lossless or structural.\n' >&2
        exit 2
        ;;
esac

cd "$repo_root"
actual_cargo_fuzz=$(cargo "+$toolchain" fuzz --version)
if [ "$actual_cargo_fuzz" != "$expected_cargo_fuzz" ]; then
    printf 'Expected %s, found %s. See %s/README.md.\n' \
        "$expected_cargo_fuzz" "$actual_cargo_fuzz" "$fuzz_dir" >&2
    exit 1
fi
cargo "+$toolchain" metadata --locked --format-version 1 > /dev/null

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/starbank-$target-fuzz.XXXXXXXX")
mkdir "$run_dir/corpus" "$run_dir/artifacts"
printf 'Fuzz corpus and artifacts are in %s\n' "$run_dir"
cp Cargo.lock "$run_dir/Cargo.lock"
python3 - "$fuzz_dir/../tests/fixtures" "$run_dir/corpus" "$target" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

fixture_dir, corpus_dir = map(Path, sys.argv[1:3])
target = sys.argv[3]
manifest = json.loads((fixture_dir / "manifest.json").read_text(encoding="utf-8"))
seeds = 0
for fixture in manifest["fixtures"]:
    if fixture["kind"] != "synthetic":
        continue
    filename = fixture["path"]
    source = fixture_dir / filename
    if Path(filename).name != filename or source.is_symlink():
        raise SystemExit(f"Invalid synthetic seed path: {filename}")
    content = source.read_bytes()
    if len(content) != fixture["bytes"] or hashlib.sha256(content).hexdigest() != fixture["sha256"]:
        raise SystemExit(f"Synthetic seed has changed: {filename}")
    (corpus_dir / filename).write_bytes(content)
    seeds += 1
if not seeds:
    raise SystemExit("No synthetic seed fixtures found")
print(f"Copied {seeds} synthetic seeds")
if target == "structural":
    edit_text = "雪🚀 <&>'\"\t\r\n".encode("utf-8")
    generated = 0
    for encoding in range(3):
        for operation in range(10):
            for alternate in range(2):
                malformed_case = (encoding + operation + alternate) % 4
                flavor = (malformed_case << 2) | 3
                flags = (flavor << 1) | alternate
                content = bytes((encoding, operation, flags)) + edit_text
                filename = f"structural-{encoding}-{operation}-{alternate}.seed"
                (corpus_dir / filename).write_bytes(content)
                generated += 1
    print(f"Generated {generated} structural seeds")
PY

status=0
cargo "+$toolchain" fuzz run --fuzz-dir "$fuzz_dir" --features fuzzing \
    --target-dir "$repo_root/target/starbank-format-fuzz" --sanitizer address \
    "$target" "$run_dir/corpus" -- \
    -seed=1 -max_len=4096 -len_control=0 -max_total_time=20 -rss_limit_mb=1024 -timeout=5 \
    -print_funcs=0 -print_final_stats=1 \
    "-artifact_prefix=$run_dir/artifacts/" || status=$?
cmp Cargo.lock "$run_dir/Cargo.lock"
exit "$status"
