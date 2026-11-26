# Bank XML under Miri

This is interpreter evidence for VO-01 in the WP-17 verification register. It uses a clean source archive of `3a0b765ad7568f4a03e86b92e349e6ea5d474807` and the repository's lockfile. The host was macOS 14.7.2 (23H311) on arm64. The verification toolchain was Rust `nightly-2026-11-04`, rustc `1.101.0-nightly (708c94ef093ecc42fb8851e17d8383d999727abc)`, and Miri `0.1.0 (708c94ef09 2026-11-03)`.

## Recorded scope

The scoped run passed 46 of the 47 existing integration cases with zero Miri diagnostics. The full invocation reported completed format (15), signature XML (6), structural (8) and UTF-16 (9) suites. A separate boundary invocation passed the remaining eight included cases. The 1,024/1,025-sibling namespace declaration case passed on native Rust 1.100.0. The full Miri invocation was interrupted after about six minutes of wall time while that case was running. The interpreter result excludes that case.

## Reproduce

Run from the repository root with the locked Cargo dependencies available locally. The test runs in an isolated export and uses Miri's default host isolation and borrow, validity and leak checks.

```sh
rustup toolchain install nightly-2026-11-04 --profile minimal --component miri --component rust-src

starbank_miri_dir=$(mktemp -d "${TMPDIR:-/tmp}/starbank-miri.XXXXXXXX")
mkdir "$starbank_miri_dir/source"
git archive 3a0b765ad7568f4a03e86b92e349e6ea5d474807 |
    LC_ALL=C tar -xf - -C "$starbank_miri_dir/source"
cd "$starbank_miri_dir/source"
export CARGO_TARGET_DIR="$starbank_miri_dir/target"
export MIRI_SYSROOT="$starbank_miri_dir/sysroot"
export CARGO_NET_OFFLINE=true
unset MIRIFLAGS
cargo +nightly-2026-11-04 miri setup --print-sysroot
/usr/bin/time -p cargo +nightly-2026-11-04 miri test --offline --locked -p sc2bank --tests -- \
    --skip namespace_declaration_budget_is_bounded_across_siblings
cargo +1.100.0 test --offline --locked -p sc2bank --test xml_boundaries -- \
    namespace_declaration_budget_is_bounded_across_siblings --exact
```

## Interpreted code

`sc2bank/src/lib.rs` forbids authored unsafe code. The command exercises the parser, UTF-8 and BOM-marked UTF-16 conversion and offset mapping, lossless serialization, signature-element XML changes, structural changes and limits in the existing synthetic tests. The interpreted crate graph includes `quick-xml` 0.42.0, `roxmltree` 0.21.2 and `memchr` 2.8.3 as reached by those cases. Native code generation, libFuzzer's C++ runtime, MPQ and profile parsing, hashing, editor and worker scheduling, native storage and UI remain outside this run. Miri checks executed paths under its memory model. Exhaustive input coverage, whole-program soundness and OS crash durability remain separate obligations.

## Other EVS-1 methods

- Kani has a tractable candidate in `sc2bank/src/offsets.rs`. A bounded harness could exercise native and working-text range conversions at UTF-16 character boundaries. It would need explicit bounds on document length and ranges, and would leave the XML parser and dependency behavior outside the proof. No Kani harness was run.
- Loom or Shuttle could explore the allocation of preparation sequence numbers under `starbank-editor/src/prepare.rs` and acknowledgement under the session's exclusive borrow. Map cancellation, worker scheduling and cache publication in `starbank-map-index/src/scan.rs` require a separate model. No scheduling model was run.
- TLC could check a finite save/backup state model connected to the phase boundaries in `starbank-fs/src/native/commit.rs`. The implementation calls the OS for locks and publication and distinguishes verified, rejected and uncertain effects. A state model would need explicit observations and crash cuts and could not qualify OS durability or externally racing writers. Existing native crash observations are recorded in `crates/platform/starbank-fs/verification/NATIVE_STORAGE.md`. No TLC model was run.
- Lean or extraction is a candidate for narrow signature-order or decimal rules. Applicability depends on inspecting the actual extraction boundary and the treatment of the involved Rust types. No Lean proof or extraction was attempted.
