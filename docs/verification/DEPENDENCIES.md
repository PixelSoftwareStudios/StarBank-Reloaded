# Locked dependency checks

On 2026-11-27, cargo-audit 0.22.2 used RustSec database commit `c5279d0b0c9e8b70df666eb6d1edbabcfd1176bd` (1,571 advisories). Node 24.22.3 and pnpm 12.23.0 checked the frontend lockfile. These are scoped dependency checks, not release or licensing approval.

| Input | Executed check | Result |
| --- | --- | --- |
| Original Cargo.lock SHA-256 `f3723fe46abe8b117617bf6a3909e25f1bac3f656df11d91e5891dc101063c54` | cargo-audit, 501 locked dependencies | No vulnerability entries. Unsound GLib API warning RUSTSEC-2024-0429; unmaintained chrono and proc-macro-error warnings. |
| Patched Cargo.lock SHA-256 `3573522707dda0e0acf8088f070e225e1bd2b244d6ca06dbe3a4f6fd579c9589` | Same database and tool, no ignored advisories | No vulnerability entries; unmaintained chrono RUSTSEC-2026-0543 and proc-macro-error RUSTSEC-2024-0370 warnings remain. |
| pnpm-lock.yaml SHA-256 `f4c357f904793787ccb6133a7c1220c54e96a09f11ed39ab286075583f0ba1c9` | `pnpm audit --json` | Registry response reported 224 dependencies and zero advisories at every severity. |
| Selected Cargo graph with all workspace features | `cargo deny --all-features list --format json --layout crate`, cargo-deny 0.20.2 | 478 inventory entries; no detected license for 18 authored packages. The remaining entries have detected expressions, not an approved distribution policy. |

## Installed frontend license inventory

On 2026-11-27, pnpm 12.23.0 reported the following installed dependency inventories on macOS arm64.

| Project | Package/version pairs | Reported license expressions |
| --- | ---: | --- |
| Workspace root | 2 | MIT OR Apache-2.0 |
| Shared UI | 72 | 0BSD, Apache-2.0, ISC, MIT |
| Web app | 61 | Apache-2.0, BSD-3-Clause, BlueOak-1.0.0, ISC, MIT, MPL-2.0 |
| Desktop app | 47 | Apache-2.0, Apache-2.0 OR MIT, BSD-3-Clause, BlueOak-1.0.0, ISC, MIT, MPL-2.0 |

The root inventory covered Biome 2.6.9 and its macOS arm64 CLI package. The three project inventories overlap and cover 127 distinct name/version pairs altogether. Every reported entry had a license expression. Other platforms' optional packages, separately bundled native components, authored package licensing, and distribution clearance remain separate review items.

## GLib iterator backport

The locked Linux desktop graph reaches GLib 0.18.5 through Tauri's GTK 3/WebKitGTK dependencies. The checked macOS, Windows, CLI and WASM graphs do not use that library. No compatible official 0.18 release contains the fix. Updating only the GLib registry version cannot satisfy the pinned GTK dependency constraints. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) · [Official fix](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34) · [Backport discussion](https://github.com/gtk-rs/gtk-rs-core/pull/1991)

The workspace now selects the exact published source with the official two-line pointer fix. Its MIT license, copyright notice and original VCS metadata are retained. The lockfile changes only the GLib source identity; package versions and other dependency edges remain unchanged. Publisher formatting is excluded from the authored Rust style gate and protected by a separate byte-integrity check. [Source and reproduction](../../crates/vendor/verification/glib/README.md)

The registry advisory scan no longer lists GLib after the path override. That result does not attest to the vendored source. The backport evidence is the exact upstream patch, the 121-file integrity check, 13 positive/negative guard tests, and 11 optimized iterator tests against GLib 2.90.0 on macOS arm64 with Rust 1.100.0. The older standalone crate emitted 41 compiler warnings. No unpatched iterator test was executed.

Hosted Ubuntu x64 and arm64 desktop jobs at `e4385da` compiled GLib 0.18.5 from `crates/vendor/glib` and passed 102 native desktop tests each. The dedicated iterator test results above were obtained on macOS arm64. Installed Linux application interaction and distribution qualification remain open. [Linux x64 job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45997318578/job/140499862244) · [Linux arm64 job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45997318578/job/140499862689)

## Unmaintained advisory review

At `2a016cf`, Cargo 1.100.0 checked lockfile SHA-256 `2fc59921afad571c75d2a6f833207e9294a7e758a8f9fd2722eb5ca7f0cb7d76`. The two reviewed RustSec entries are informational unmaintained-package warnings with no patched version listed. This graph review does not suppress either warning or approve release licensing.

`chrono 0.4.45` is locked through an optional `serde_with 3.24.0` dependency, but no checked workspace graph activates it, including all workspace features and all targets. `tauri-utils 2.10.4` requests `serde_with` with its default features. The resolved feature set is alloc, default, macros and std; the optional Chrono feature stays off. Keep the warning visible and repeat this check when feature selection changes. [RustSec warning](https://rustsec.org/advisories/RUSTSEC-2026-0543.html) · [Pinned feature definitions](https://raw.githubusercontent.com/jonasbb/serde_with/v3.24.0/serde_with/Cargo.toml)

`proc-macro-error 1.0.4` is active during Linux x64 and arm64 desktop compilation and desktop-feature tests. The pinned GTK graph reaches it through both `gtk3-macros 0.18.2` and vendored GLib's `glib-macros 0.18.5`. Excluding procedural-macro edges removes both paths. It is absent from the checked macOS, Windows, CLI and wasm32 graphs. This describes host compilation exposure; procedural macros execute during compilation. Every published 0.18 release of those two macro crates still requires version 1. Their 0.19 releases drop it but fall outside the current GTK constraints. Reassess with a coordinated upstream Linux desktop update rather than deleting lock entries or making an incompatible pin change. [RustSec warning](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) · [GLib macro versions](https://raw.githubusercontent.com/rust-lang/crates.io-index/master/gl/ib/glib-macros) · [GTK macro versions](https://raw.githubusercontent.com/rust-lang/crates.io-index/master/gt/k3/gtk3-macros) · [Procedural-macro execution](https://doc.rust-lang.org/reference/procedural-macros.html)

```sh
cargo tree --locked --offline --workspace --all-features --target all \
  -e normal,build,dev -i chrono@0.4.45
cargo tree --locked --offline --workspace --all-features --target all \
  -e features -i serde_with@3.24.0 --depth 3
cargo tree --locked --offline -p starbank-desktop --features desktop,custom-protocol \
  --target x86_64-unknown-linux-gnu -e normal,build -i proc-macro-error@1.0.4
cargo tree --locked --offline -p starbank-desktop --features desktop,custom-protocol \
  --target x86_64-unknown-linux-gnu -e normal,build,no-proc-macro -i proc-macro-error@1.0.4
```

The commands returned an empty Chrono tree, the four `serde_with` features above, both Linux macro paths, and an empty tree after excluding procedural macros. The Linux result also held for `aarch64-unknown-linux-gnu`. No dependencies, features or lockfile entries changed.

## Reproduction and remaining gates

Use the pinned tools and a fresh RustSec database for a new audit. Record the database revision and lock hashes again when either changes.

```sh
cargo audit --file Cargo.lock --deny unsound
pnpm audit --json
pnpm licenses list --json
pnpm --dir packages/starbank-ui licenses list --json
pnpm --dir apps/starbank-web licenses list --json
pnpm --dir apps/starbank-desktop licenses list --json
cargo deny --all-features list --format json --layout crate
python3 crates/vendor/verification/glib/check.py
python3 crates/vendor/verification/glib/test_check.py
```

The unmaintained dependency warnings retain the scoped review above and require reassessment when their feature or upstream dependency graphs change. Cargo inventory does not cover the separately linked opt-in native StormLib components or approve their notices. Asset rights, the authored release license, package signing and final distribution clearance remain open. [Native component inventory](../../crates/adapters/starbank-mpq-stormlib/verification/NATIVE_APPEND.md) · [Distribution evidence](DISTRIBUTION.md)

The CI source job verifies the vendored bytes and its negative tests before running pinned cargo-audit 0.22.2 with `--deny unsound`. The frontend job runs the default `pnpm audit` immediately after its frozen install. The exact online commands passed locally against the recorded database and lockfiles; the original Rust lock failed the unsoundness gate. No advisory exception was added. The hosted source job at `e4385da` verified 121 vendored files and passed 13 guard tests before auditing 501 locked Rust dependencies against 1,571 advisories. The two unmaintained warnings remained. The frontend advisory gate reported no known vulnerabilities. [Source job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45997318578/job/140499862302) · [Frontend job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45997318578/job/140499862429)
