# Native MPQ append qualification

This optional WP-09 backend covers stored, neutral-locale append to a bounded classic V1 archive. It takes an immutable `V1AppendPlan` from the independent Rust verifier and returns candidate bytes only after checking that same plan against the result. REQ-MPQ-02 still needs authorized SC2 component semantics, real archive coverage, and target editor acceptance. The feature is disabled by default and the desktop repair capability remains unavailable.

## Rebuild the static dependency

Use Rust 1.100.0, native arm64 macOS, CMake 4.4.4, and Xcode 16.2 build 16C5032a with SDK 15.2. The bootstrap takes a separately acquired source tarball and checks its SHA-256 before opening it. StormLib v9.40 is commit `6bb1882bd00ddbc3729cac5dac0fda81a61e5514` and the official tag tarball has SHA-256 `f80b08bae168888702a4251312d6c2279f487673df611c2b5ef4f395e810c3b1`. [Pinned source](https://github.com/ladislav-zezula/StormLib/tree/6bb1882bd00ddbc3729cac5dac0fda81a61e5514) · [Source tarball](https://codeload.github.com/ladislav-zezula/StormLib/tar.gz/refs/tags/v9.40)

```sh
STARBANK_STORMLIB_ARTIFACT="$(
  crates/adapters/starbank-mpq-stormlib/verification/bootstrap-macos-arm64.sh ./StormLib-v9.40.tar.gz
)"
export STARBANK_STORMLIB_ARTIFACT
cargo test --locked --offline -p starbank-mpq-stormlib --features native-stormlib
cargo clippy --locked --offline -p starbank-mpq-stormlib --all-targets --features native-stormlib -- -D warnings
cargo doc --locked --offline -p starbank-mpq-stormlib --no-deps --features native-stormlib
```

CMake builds the static archive with bundled code and a macOS 14.0 deployment target. The bootstrap checks that the archive reports only arm64 and produces an artifact directory with the archive and a source, target, and archive digest stamp. Cargo checks the stamp and digest and links the checked bytes under a digest-qualified name in its private output directory. It needs an explicit artifact directory from the trusted local bootstrap, with no fetch or system StormLib lookup. On this host, two fresh scratch builds with the pinned inputs produced byte-identical archives with SHA-256 `56f86391735e19a8678dbfda9d796a375e12db5d12d22a3dca5b1ac3b441c1d2`. The C compiler reported upstream deprecation warnings. [CMake inputs](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/CMakeLists.txt)

## FFI and filesystem boundary

- The input is a pure Rust plan checked before any bytes enter C. Only the fixed local APFS scratch root under `/private/tmp` is eligible. A generated private directory and exclusively created regular file hold the copy. Only its fixed-shape absolute pathname enters StormLib. Provider syntax, mirror syntax, and caller-selected paths cannot reach that argument. [Pinned stream parsing](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/FileStream.cpp#L1138-L1158)
- The binding uses the pinned ABI and synchronous calls. Each non-null archive or write handle has a single Rust owner. A member borrows its archive exclusively. The native open disables header scanning beyond offset zero. The stored write passes zero flags. Its first nonempty buffer is one byte, below the pinned incremental-patch detector’s minimum. Remaining bytes are copied into StormLib’s allocated sector buffer, preserving arbitrary input alignment and patch-like payloads as ordinary stored data. StormLib applies EXISTS internally and treats that same bit as REPLACEEXISTING in the incoming flags. Finish consumes the write handle even when an incomplete payload fails. Flush and close return checked status before readback. Any failure prevents returning the copy. If candidate creation and scratch cleanup both fail, the error retains both diagnostics. A private mutex serializes calls including cleanup because StormLib initializes global tables without synchronization. [Pinned create flags](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/StormLib.h#L226-L246) · [Pinned open flags](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/StormLib.h#L339-L350) · [Pinned create ABI](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/StormLib.h#L1123-L1128) · [Handle finalization](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/SFileAddFile.cpp#L719-L765) · [Shared hash state](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/SBaseCommon.cpp#L209-L234)
- Readback uses the adapter-owned file descriptor and the plan's byte ceiling. The Rust verifier checks the candidate's geometry, retained packed bytes and exact new member payloads. The adapter creates no output at a caller-selected location. Fault containment for defects inside the native library and qualification against broader input classes remain open.

## Observed headless results

On macOS 14.7.2 arm64, the integration test read `v1-no-list.s2ma`, appended two synthetic entries, verified the candidate through the independent Rust parser and reread the source fixture byte-for-byte. Its three retained hash slots, block records and packed data remained identical. A separate read-only StormLib process reopened that candidate, checked both new entries against their supplied payloads, and compared all three original member payloads against the source.

| Archive | Bytes | SHA-256 |
| --- | ---: | --- |
| Source and repeated source read | 11,143 | `bbf6555dd7560c2b925c6662725dee358507a00ad9b2bf15c0c1cab30205ca14` |
| Candidate with two appended members | 11,235 | `427b0ca5cdbd331d378acdae18d53ebd0970ed1424f3e8069317ce9fb37ccb5e` |

The crate tests also exercised attempted native duplicate append, direct create over an existing member with byte-identical readback, rejection of a displaced header, incomplete member finalization, scratch path validation, and exact stored append of an unaligned patch-like payload and a single-byte member. The bootstrap rejected an incorrect source tarball. The Cargo build rejected an absent artifact, a changed archive with the original stamp, and native feature activation for a different target. Default-feature builds passed on arm64 macOS, wasm32 and Windows MSVC. This proof used repository-authored synthetic archives only. Miri cannot execute StormLib or the filesystem calls.

Windows, Linux and Intel macOS native linking and execution remain unqualified. The top-level StormLib MIT notice applies alongside the bundled component notices. Before distributing the static library or an application containing it, resolve notice and provenance obligations for the exact bundled bzip2, zlib, LibTomCrypt, LibTomMath, LZMA, lookup3, and PKLIB sources. PKLIB source contains an embedded PKWARE copyright marker, and exact vintage LibTomMath provenance remains open. [Upstream license](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/LICENSE) · [Bundled PKLIB source](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/pklib/explode.c#L20-L29)
