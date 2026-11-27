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

Windows, Linux and Intel macOS native linking and execution remain unqualified.

## Bundled native dependency inventory

The macOS arm64 archive with SHA-256 `56f86391735e19a8678dbfda9d796a375e12db5d12d22a3dca5b1ac3b441c1d2` contains 242 C/C++ object members plus its archive symbol index. Each member matches a source selected by the pinned CMake configuration, counting both `compress.c.o` members. The build selects bundled zlib and bzip2. On macOS `WITH_LIBTOMCRYPT=OFF` disables system LibTomCrypt lookup and selects the bundled LibTomCrypt and LibTomMath source lists. [CMake sources](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/CMakeLists.txt#L23-L55) · [CMake dependencies](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/CMakeLists.txt#L266-L331)

| Component | Archive objects | Source notice and provenance |
| --- | ---: | --- |
| StormLib v9.40 core and bundled ADPCM, Huffman and sparse code | 23 | Ladislav Zezula's top-level MIT notice. [Pinned license](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/LICENSE) |
| zlib 1.2.5 | 9 | Jean-loup Gailly and Mark Adler's zlib notice is in the bundled header. [Pinned header](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/zlib/zlib.h#L1-L24) |
| bzip2 1.0.5 | 7 | Julian Seward's bundled source identifies this version and refers to its license. The full text is in the 1.0.5 manual. [Pinned source](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/bzip2/bzlib.c#L7-L19) · [Versioned manual](https://sourceware.org/bzip2/1.0.5/bzip2-manual-1.0.5.html) |
| LZMA SDK | 3 | The 2009 headers in `LzFind.c`, `LzmaDec.c` and `LzmaEnc.c` name Igor Pavlov and state public domain. The precise SDK release was not identified. [Pinned source directory](https://github.com/ladislav-zezula/StormLib/tree/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/lzma/C) |
| lookup3, May 2006 | 1 | Bob Jenkins's bundled source states public domain. [Pinned source](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/jenkins/lookup3.c#L1-L9) |
| LibTomCrypt 1.17 and StormLib integration | 79 | The bundled header reports 1.17. The original tag carries Tom St Denis's public-domain declaration. The archive includes 77 units from the bundled list plus two units with Zezula integration provenance, `rsa_verify_simple.c` and `crypt_libc.c`. [Pinned version header](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/libtomcrypt/src/headers/tomcrypt.h#L18-L20) · [Original tag notice](https://raw.githubusercontent.com/libtom/libtomcrypt/1.17/LICENSE) |
| LibTomMath 0.41-derived subset | 118 | All 121 bundled C and header files match the corresponding 0.41 tag files after normalizing CRLF, local include delimiters and expanded CVS comments. The tag carries Tom St Denis's public-domain declaration. This comparison identifies the source baseline. [0.41 tag notice](https://raw.githubusercontent.com/libtom/libtommath/333aebc89409194a49d8c47628ed70bdfabfb648/LICENSE) |
| PKLIB implode and explode | 2 | Both files carry Zezula's copyright header. `explode.c` also embeds a PKWARE Data Compression Library 1.11 copyright marker. The marker is present in the static archive. PKLIB source provenance and applicable notice treatment remain unresolved. [Pinned explode source](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/pklib/explode.c#L1-L29) · [Pinned implode source](https://github.com/ladislav-zezula/StormLib/blob/6bb1882bd00ddbc3729cac5dac0fda81a61e5514/src/pklib/implode.c#L1-L11) |

For this build, `ar -t` established archive membership. `xcrun nm -gU` on the opt-in native integration-test executable found defined bzip2, zlib, LZMA, lookup3, PKLIB, LibTomCrypt and LibTomMath symbols. Those same symbols were absent in the default-feature test executable. `otool -L` on the opt-in test executable listed only macOS system runtime libraries. These checks describe the static archive and the tested link. Installer and release executable contents remain unverified.

Use the pinned source notices in a third-party notice inventory for any release containing this artifact. Record the final product link separately and retain the PKLIB provenance question in the release record.
