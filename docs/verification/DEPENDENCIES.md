# Locked dependency checks

On 2026-11-27, cargo-audit 0.22.2 used RustSec database commit `c5279d0b0c9e8b70df666eb6d1edbabcfd1176bd` (1,571 advisories). Node 24.22.3 and pnpm 12.23.0 checked the frontend lockfile. These are scoped dependency checks, not release or licensing approval.

| Input | Executed check | Result |
| --- | --- | --- |
| Original Cargo.lock SHA-256 `f3723fe46abe8b117617bf6a3909e25f1bac3f656df11d91e5891dc101063c54` | cargo-audit, 501 locked dependencies | No vulnerability entries. Unsound GLib API warning RUSTSEC-2024-0429; unmaintained chrono and proc-macro-error warnings. |
| Patched Cargo.lock SHA-256 `3573522707dda0e0acf8088f070e225e1bd2b244d6ca06dbe3a4f6fd579c9589` | Same database and tool, no ignored advisories | No vulnerability entries; unmaintained chrono RUSTSEC-2026-0543 and proc-macro-error RUSTSEC-2024-0370 warnings remain. |
| pnpm-lock.yaml SHA-256 `f4c357f904793787ccb6133a7c1220c54e96a09f11ed39ab286075583f0ba1c9` | `pnpm audit --json` | Registry response reported 224 dependencies and zero advisories at every severity. |
| Selected Cargo graph with all workspace features | `cargo deny --all-features list --format json --layout crate`, cargo-deny 0.20.2 | 478 inventory entries; no detected license for 18 authored packages. The remaining entries have detected expressions, not an approved distribution policy. |

## GLib iterator backport

The locked Linux desktop graph reaches GLib 0.18.5 through Tauri's GTK 3/WebKitGTK dependencies. The checked macOS, Windows, CLI and WASM graphs do not use that library. No compatible official 0.18 release contains the fix. Updating only the GLib registry version cannot satisfy the pinned GTK dependency constraints. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) · [Official fix](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34) · [Backport discussion](https://github.com/gtk-rs/gtk-rs-core/pull/1991)

The workspace now selects the exact published source with the official two-line pointer fix. Its MIT license, copyright notice and original VCS metadata are retained. The lockfile changes only the GLib source identity; package versions and other dependency edges remain unchanged. Publisher formatting is excluded from the authored Rust style gate and protected by a separate byte-integrity check. [Source and reproduction](../../crates/vendor/verification/glib/README.md)

The registry advisory scan no longer lists GLib after the path override. That result does not attest to the vendored source. The backport evidence is the exact upstream patch, the 121-file integrity check, 13 positive/negative guard tests, and 11 optimized iterator tests against GLib 2.90.0 on macOS arm64 with Rust 1.100.0. The older standalone crate emitted 41 compiler warnings. The Linux application dependency tree selects the backport, but Linux application build/runtime and distribution qualification remain open; the local Docker daemon was unavailable. No unpatched iterator test was executed.

## Reproduction and remaining gates

Use the pinned tools and a fresh RustSec database for a new audit. Record the database revision and lock hashes again when either changes.

```sh
cargo audit --file Cargo.lock --deny unsound
pnpm audit --json
cargo deny --all-features list --format json --layout crate
python3 crates/vendor/verification/glib/check.py
python3 crates/vendor/verification/glib/test_check.py
```

The unmaintained dependency warnings still need scoped review. Cargo inventory does not cover the separately linked opt-in native StormLib components or approve their notices. Asset rights, the authored release license, package signing, hosted execution and final distribution clearance remain open. [Native component inventory](../../crates/adapters/starbank-mpq-stormlib/verification/NATIVE_APPEND.md) · [Distribution evidence](DISTRIBUTION.md)

The CI source job verifies the vendored bytes and its negative tests before running pinned cargo-audit 0.22.2 with `--deny unsound`. The frontend job runs the default `pnpm audit` immediately after its frozen install. The exact online commands passed locally against the recorded database and lockfiles; the original Rust lock failed the unsoundness gate. No advisory exception was added. Hosted job results are not available under the configured GitHub CLI account.
