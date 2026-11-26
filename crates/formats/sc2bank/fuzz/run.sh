#!/bin/sh
set -eu

fuzz_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$fuzz_dir/../../../.." && pwd)
toolchain=nightly-2026-11-04
expected_cargo_fuzz='cargo-fuzz 0.13.2'

cd "$repo_root"
actual_cargo_fuzz=$(cargo "+$toolchain" fuzz --version)
if [ "$actual_cargo_fuzz" != "$expected_cargo_fuzz" ]; then
    printf 'Expected %s, found %s. See %s/README.md.\n' \
        "$expected_cargo_fuzz" "$actual_cargo_fuzz" "$fuzz_dir" >&2
    exit 1
fi
cargo "+$toolchain" metadata --locked --format-version 1 > /dev/null

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/starbank-lossless-fuzz.XXXXXXXX")
mkdir "$run_dir/corpus" "$run_dir/artifacts"
printf 'Fuzz corpus and artifacts are in %s\n' "$run_dir"
cp Cargo.lock "$run_dir/Cargo.lock"
python3 - "$fuzz_dir/../tests/fixtures" "$run_dir/corpus" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

fixture_dir, corpus_dir = map(Path, sys.argv[1:])
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
PY

status=0
cargo "+$toolchain" fuzz run --fuzz-dir "$fuzz_dir" --features fuzzing \
    --target-dir "$repo_root/target/starbank-format-fuzz" --sanitizer address \
    lossless "$run_dir/corpus" -- \
    -seed=1 -max_len=4096 -len_control=0 -max_total_time=20 -rss_limit_mb=1024 -timeout=5 \
    -print_funcs=0 -print_final_stats=1 \
    "-artifact_prefix=$run_dir/artifacts/" || status=$?
cmp Cargo.lock "$run_dir/Cargo.lock"
exit "$status"
