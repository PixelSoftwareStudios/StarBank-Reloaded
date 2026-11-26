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
```

The script defaults to `lossless`. It verifies each synthetic fixture against its manifest's byte count and SHA-256 before copying it to a fresh temporary corpus. The structural run adds 60 generated seeds covering both target variants of each operation in all three encodings. It uses a fixed libFuzzer seed, a 4,096-byte input cap, a 20-second fuzzing budget, a five-second per-input timeout, and a 1,024 MiB RSS cutoff. The scalar and structural targets use at most 256 and 96 input bytes per edit, respectively. LibFuzzer reports final counts and writes its generated corpus and any failure artifacts to the printed temporary directory. The harness uses the workspace lockfile and builds only with the explicit `fuzzing` feature.

## Recorded smoke

On November 26, 2026, the script ran against `b6c4a6ef56dde5ff5226f15026eff1a1d9fbb1cb` with this harness applied. macOS 14.7.2 arm64 used rustc `1.101.0-nightly (708c94ef0)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13. The run loaded all 10 synthetic seeds and completed 61,970 inputs in 21 seconds with a reported peak RSS of 421 MiB. It exited successfully and produced no assertion failure, sanitizer report or crash artifact. The workspace lockfile was unchanged after the run.

On November 27, 2026, `run.sh structural` ran against `e2cf03533264146e23815f38f1353c2d5c829555` with the structural harness applied. On macOS 14.7.2 arm64, rustc `1.101.0-nightly (708c94ef0)`, cargo-fuzz 0.13.2 and libfuzzer-sys 0.4.13 completed 70,622 inputs in 21 seconds. The run loaded 70 synthetic seeds and reported 455 MiB peak RSS. It exited successfully with zero assertion failures, sanitizer reports or crash artifacts. The workspace lockfile was unchanged.

Sustained runs and separate fuzz targets for the other formats remain pending.
