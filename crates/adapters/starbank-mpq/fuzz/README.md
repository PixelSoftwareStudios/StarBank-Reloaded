# Classic MPQ V1 append fuzz target

`v1_append` checks the pure preflight and preservation verifier for WP-09 and REQ-MPQ-02. It tests arbitrary source and candidate bytes, then uses the crate's [synthetic archive pair](../tests/fixtures/README.md) to reach deeper checks. The target changes retained and added payloads, alters decoded hash and block table entries, varies portable member names, and changes the proposed addition bytes. Its assertions cover exact opaque-byte retention, rejection of changed retained records or mismatched additions, acceptance of case-equivalent names, and rejection of a traversal component. The target performs no file extraction or writes.

## Run on a native AddressSanitizer host

Run the commands from the repository root with a C++ toolchain and Rustup. The harness accepts at most 4,096 input bytes and applies at most 32 mutations to its bounded synthetic fixtures per input.

```sh
rustup toolchain install nightly-2026-11-04 --profile minimal
cargo +1.100.0 install cargo-fuzz --version '=0.13.2' --locked

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/starbank-mpq-v1-fuzz.XXXXXX")
mkdir "$run_dir/corpus" "$run_dir/artifacts"
cp Cargo.lock "$run_dir/Cargo.lock"
cp crates/adapters/starbank-mpq/tests/fixtures/append-v1-source.s2ma \
  crates/adapters/starbank-mpq/tests/fixtures/append-v1-output.s2ma "$run_dir/corpus/"
for selector in 000 001 002 003 004 005 006 007; do
  printf "\\$selector\\000\\001\\002" > "$run_dir/corpus/operation-$selector"
done
printf '\006\001x' > "$run_dir/corpus/traversal-name"
printf '\006\002Edited.txt' > "$run_dir/corpus/arbitrary-name"

cargo +nightly-2026-11-04 fuzz run \
  --fuzz-dir crates/adapters/starbank-mpq/fuzz --features fuzzing \
  --target-dir target/starbank-mpq-fuzz --sanitizer address v1_append "$run_dir/corpus" -- \
  -seed=1 -max_len=4096 -len_control=0 -max_total_time=90 -rss_limit_mb=1024 \
  -timeout=5 -print_funcs=0 -print_final_stats=1 "-artifact_prefix=$run_dir/artifacts/"
cmp Cargo.lock "$run_dir/Cargo.lock"
```

LibFuzzer writes generated corpus inputs and any failures into the temporary run directory. The target is opt-in and uses the workspace lockfile.

## Recorded run

On November 27, 2026, the target ran from an archive of `854bfef` with the V1 append verifier and this fuzz target applied. AddressSanitizer and libFuzzer completed 3,526,106 executions from 12 synthetic seeds in 91 seconds on macOS 14.7.2 arm64. The run reported 466 MiB peak RSS and exited successfully without an assertion failure, sanitizer report or failure artifact. It used rustc `1.101.0-nightly (708c94ef0)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13. The workspace lockfile was unchanged during the run.
