# Bank XML fuzz targets

The `lossless` target contributes evidence for AT-FMT-04 and VO-01. It parses arbitrary byte slices within a fixed budget and checks that every accepted bank exports byte-for-byte unchanged. For each nonempty input, the first byte selects a known value from an authored fixture in UTF-8, UTF-16LE or UTF-16BE. The remaining bytes generate bounded XML text for a targeted edit. The target checks byte preservation outside the attribute's original contents, an unchanged source document, and matching encoding, occurrence ID, type and decoded value after reparsing.

The `structural` target covers all section, key and value add, remove and rename operations and value-type changes. It compares the complete output bytes against an authored XML expectation in each supported encoding. This checks unaffected XML, including BOM, line endings, duplicate names, comments, opaque namespaces and the stored signature. It also reparses the output and checks that occurrence remapping retains surviving IDs in order, drops removed descendants and locates the added occurrence under the requested parent. The target verifies typed refusal of partial UTF-16 code units, unpaired high and low surrogates and contradictory declarations in both byte orders. It checks that an invalid structural name leaves the source unchanged.

## Run locally

Use a native AddressSanitizer-supported host with Python 3, a C++ toolchain and Rustup. From the repository root, install the pinned verification tools and run the script.

```sh
rustup toolchain install nightly-2026-11-04 --profile minimal
cargo +1.100.0 install cargo-fuzz --version '=0.13.2' --locked
sh crates/formats/sc2bank/fuzz/run.sh lossless
sh crates/formats/sc2bank/fuzz/run.sh structural
sh crates/formats/sc2bank/fuzz/run.sh lossless 90
sh crates/formats/sc2bank/fuzz/run.sh structural 90
```

The script defaults to `lossless` and 20 seconds. Pass a duration from 1 to 600 seconds as the second argument. It verifies each synthetic fixture against its manifest's byte count and SHA-256 before copying it to a fresh temporary corpus. The scalar run adds 12 generated raw UTF-16 seeds covering both byte orders, generic and explicit declarations, odd lengths, unpaired surrogates and conflicting declarations. The structural run adds its 60 generated seeds covering both target variants of each operation in all three encodings. The script uses a fixed libFuzzer seed, a 4,096-byte input cap, a five-second per-input timeout, and a 1,024 MiB RSS cutoff. The scalar and structural targets use at most 256 and 96 input bytes per edit, respectively. LibFuzzer reports final counts and writes its generated corpus and any failure artifacts to the printed temporary directory. The harness uses the workspace lockfile and builds only with the explicit `fuzzing` feature.

## Recorded smoke

On November 26, 2026, the script ran against `b6c4a6ef56dde5ff5226f15026eff1a1d9fbb1cb` with this harness applied. macOS 14.7.2 arm64 used rustc `1.101.0-nightly (708c94ef0)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13. The run loaded all 10 synthetic seeds and completed 61,970 inputs in 21 seconds with a reported peak RSS of 421 MiB. It exited successfully and produced no assertion failure, sanitizer report or crash artifact. The workspace lockfile was unchanged after the run.

On November 27, 2026, `run.sh structural` ran against `e2cf03533264146e23815f38f1353c2d5c829555` with the structural harness applied. On macOS 14.7.2 arm64, rustc `1.101.0-nightly (708c94ef0)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13 completed 70,622 inputs in 21 seconds. The run loaded 70 synthetic seeds and reported 455 MiB peak RSS. It exited successfully with zero assertion failures, sanitizer reports or crash artifacts. The workspace lockfile was unchanged.

## Recorded 90-second runs

On November 27, 2026, both unchanged targets ran from a clean archive of `3855597143f3d4c3677b01e1b953905a1f6fb38c` on macOS 14.7.2 arm64. The scratch runner used the same seeds and 90-second budgets as the commands above. AddressSanitizer and libFuzzer completed 267,922 scalar executions from 22 seeds and 271,068 structural executions from 70 seeds, each in 91 seconds. Both exited successfully with no assertion failure, sanitizer report or failure artifact. The workspace lockfile stayed byte-identical. Rustc `1.101.0-nightly (708c94ef0)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13 were used.

The maintained scalar command also ran with its default 20-second budget and all 22 seeds. It completed 68,940 executions in 21 seconds with zero failures.

Longer sanitizer campaigns and targets for the other formats remain pending.
