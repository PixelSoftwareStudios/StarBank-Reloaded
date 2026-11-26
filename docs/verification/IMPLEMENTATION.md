# Initial implementation evidence

## WP-03 bank XML

`sc2bank` has a bounded UTF-8 XML 1.0 parser and serializer. It keeps original source bytes, ordered section/key/value occurrences and unknown markup. Export without changes returns the original bytes. Edits to recognized existing value attributes replace their original byte ranges, escape the inserted values and revalidate the result. Unsupported encodings, DTDs, invalid references, ambiguous edits and exceeded limits return typed errors.

Rust 1.100.0 passed 15 synthetic tests on macOS arm64 and built the crate for `wasm32-unknown-unknown`. The tests cover BOM and line endings, duplicate names, multiple and missing values, exact numeric lexemes, namespace discrimination, opaque markup, processing-instruction syntax and bounded failure cases. All 10 fixture digests matched. Structural editing and signature mutation belong to later slices. [Tests](../../crates/formats/sc2bank/tests/format.rs) · [Fixture provenance](../../crates/formats/sc2bank/tests/fixtures/README.md)

An isolated AddressSanitizer and libFuzzer smoke ran 114,685 inputs in 31 seconds at `8fbe402`, seeded from the 10 synthetic fixtures. It checked byte-identical unchanged export, bounded parsing and targeted edits followed by reparse and byte comparisons. There were no crashes, sanitizer errors or assertion failures. The run used cargo-fuzz 0.13.2, libfuzzer-sys 0.4.13 and Rust nightly-2026-11-04. A checked-in harness and sustained fuzz jobs remain pending.

## WP-04 signature computation and verification

`starbank-signature` validates explicit author, player and bank-name context, computes the SHA-1 digest using stable UTF-16 ordinal ordering, and checks a stored signature without mutating the bank. It distinguishes unsigned, malformed, missing-context, unsupported, valid and invalid states. Eight Rust tests passed on macOS arm64. The WASM library built on Rust 1.100.0. [Tests](../../crates/domain/starbank-signature/tests/signatures.rs)

An isolated harness linked original C# source at `1f95ab3425753b12d6387d327870dc6d408c49ac`. Four synthetic vectors matched the pinned signer on .NET 10.0.12 on macOS, including ordering, stable ties, decoded XML whitespace and supplementary Unicode. Three vectors also matched after upstream XML write and reread. The harness reported two upstream SHA-1 API deprecation warnings. Windows .NET Framework execution, game-generated bank conformance and mutation through prepared re-sign/export are pending. [Fixture provenance](../../crates/domain/starbank-signature/tests/fixtures/README.md) · [Pinned signer](https://github.com/BlueRaja/Starbank/blob/1f95ab3425753b12d6387d327870dc6d408c49ac/StarBank/Bank%20Stuffs/BankSignatureGenerator.cs#L11-L47)

## WP-08 native discovery

`starbank-paths` resolves prioritized macOS and Windows candidates, with an explicit Linux automatic-discovery stub. Manual selection works independently of automatic detection. Scoped inventory distinguishes live banks and backups, includes hidden banks and keeps distinct same-name files. It retains hard-link locations and infers signing hints only from recognized account/player/author anchors. Ten macOS tests passed, and Windows MSVC cross-target Clippy passed. Real Windows installations, redirected folders, junction behavior and player display names remain to be verified. [Tests](../../crates/discovery/starbank-paths/tests/paths.rs)

## WP-11 map sidebar

The shared shell consumes typed inventory and selection callbacks. The map list supports case-insensitive search, independent Blizzard and bank-presence filters, and distinct empty states. Filter changes preserve caller-owned selection. A desktop separator starts at 28% and supports keyboard and pointer resizing. On narrow layouts the map list stacks below the workspace, and Quick Options stays below the scrolling list.

The production build and four unit tests passed with the pinned frontend toolchain. Synthetic and empty-state browser checks passed in Chromium 159, Firefox 159 and WebKit 26.5 at 850×500 dark and 390×640 light, with no off-origin requests or page errors. WebKit used Playwright 1.61.1 because the 1.65.1 macOS 14 driver cannot open the page. Native inventory, persisted settings, breadcrumbs, metadata and tool actions remain open. [Test](../../apps/starbank-web/src/app-shell.test.tsx) · [WebKit driver issue](https://github.com/microsoft/playwright/issues/42964)

## Integrated gates

At `8fbe402` on macOS arm64, the workspace passed 41 Nextest cases, formatting, Clippy with warnings denied, rustdoc and the source layout check. Core, format and signing built for wasm32 and passed target-specific Clippy. The workspace passed Windows MSVC cross-target Clippy. Actionlint and the archive and requirements validator passed. The RustSec lockfile scan exited successfully with zero reported advisories for this dependency set. [RustSec snapshot](https://github.com/RustSec/advisory-db/commit/c9f6c91a44dbad9d71742329e9df061279f9c4be) · [Go archive](../reference/GO_INVENTORY.md) · [Bootstrap evidence](BOOTSTRAP.md)

On Debian 12 Linux arm64, all 42 workspace tests passed at `8fbe402`, including the Linux discovery stub and permission-denied behavior, as an unprivileged account without Linux capabilities. The run mounted a read-only export of that commit and used Rust 1.100.0 from official image digest `sha256:bb875b0f8ce726053e259034848c471840e96bfe591fd52556178ae9a05e6126`. It fetched the locked dependencies, then ran `cargo test --locked --offline --workspace`. Linux GUI and x86_64 runtime remain unverified. [Official Rust container image](https://hub.docker.com/_/rust)
