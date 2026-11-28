# Locked dependency checks

The Windows Save As navigation/close integration enables `common-controls-v6` on the existing pinned RFD 0.17.2 dependency. Locked Windows feature-graph inspection and cross-target Clippy confirmed the feature; Cargo.lock is unchanged. RFD uses native TaskDialog custom labels with the Common Controls v6 manifest already supplied by tauri-build. This is a feature change rather than a package upgrade and leaves Linux GTK selection intact. Windows native test linkage, all 109 desktop tests and silent NSIS build/install/uninstall checks passed at `6998624`; actual Windows dialogs remain unverified. [Implementation and scope](IMPLEMENTATION.md#windows-save-as-before-navigation-or-close)

On 2026-11-27, cargo-audit 0.22.2 used RustSec database commit `c5279d0b0c9e8b70df666eb6d1edbabcfd1176bd` (1,571 advisories). Node 24.22.3 and pnpm 12.23.0 checked the frontend lockfile. These are scoped dependency checks, not release or licensing approval.

| Input | Executed check | Result |
| --- | --- | --- |
| Original Cargo.lock SHA-256 `f3723fe46abe8b117617bf6a3909e25f1bac3f656df11d91e5891dc101063c54` | cargo-audit, 501 locked dependencies | No vulnerability entries. Unsound GLib API warning RUSTSEC-2024-0429; unmaintained chrono and proc-macro-error warnings. |
| Patched Cargo.lock SHA-256 `3573522707dda0e0acf8088f070e225e1bd2b244d6ca06dbe3a4f6fd579c9589` | Same database and tool, no ignored advisories | No vulnerability entries; unmaintained chrono RUSTSEC-2026-0543 and proc-macro-error RUSTSEC-2024-0370 warnings remain. |
| pnpm-lock.yaml SHA-256 `f4c357f904793787ccb6133a7c1220c54e96a09f11ed39ab286075583f0ba1c9` | `pnpm audit --json` | Registry response reported 224 dependencies and zero advisories at every severity. |
| Selected Cargo graph with all workspace features | `cargo deny --all-features list --format json --layout crate`, cargo-deny 0.20.2 | 478 inventory entries; no detected license for 18 authored packages. The remaining entries have detected expressions, not an approved distribution policy. |

## GTK native chooser reference backport

Pinned RFD 0.17.2 destroys its `GtkFileChooserNative` in `Drop` but does not release the constructor's owned GObject reference. GTK documents that native-dialog destroy does not release references; chooser finalization destroys the retained child window. This is a source-level ownership leak, distinct from a measured heap-growth claim. The hidden title observed in the failed cancellation fixture led to this review. [GTK constructor ownership](https://docs.gtk.org/gtk3/ctor.FileChooserNative.new.html) · [Destroy contract](https://docs.gtk.org/gtk3/method.NativeDialog.destroy.html)

Released RFD 0.18.0 at `2d35d7d` still omits that unref; its returned-filename/list fixes do not solve the native-dialog reference. At inspection, upstream fix [PR 366](https://github.com/PolyMeilex/rfd/pull/366), head `eedc63964337fef1f49dd043cc4e52a93dc96c9e`, was open, unmerged and unreviewed. The initial backport at `912ea6d` preserved all 61 files from the exact 0.17.2 release, its MIT/portal notices and publisher formatting, changing only the chooser Drop implementation. Its original offline integrity gate verified all bytes and the single patch; ten mutation tests passed. A lock comparison confirmed every package version is unchanged and only RFD's source identity changed to the checked path. [Backport reproduction](../../crates/vendor/verification/rfd/README.md)

An independent ownership review checked both sync and async paths. Normal Drop remains on the GTK execution path; GLib retains the signal instance and closure during response handling, so releasing the caller-owned reference after dispose does not invalidate the executing callback. Repeated async Cancel/Accept must leave no titled chooser X window, including hidden ones, after the editor has settled. `G_DEBUG=fatal-criticals` is fixture-only. Both installed Linux architectures passed the teardown regression at `912ea6d`, including Cancel and Accept while the app remained alive. All eleven jobs passed. No heap-growth or whole-future-cancellation proof is claimed. [Qualified run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46094590533) Returned filename/list allocations are separate from that reference fix; the follow-up is described below. The 0.18 non-UTF8 behavior is not included.

Local desktop tests, formatting, source-check tests, macOS Clippy and Windows cross-target Clippy passed. The unchanged macOS backend emits 50 publisher warnings when compiled as a path dependency (deprecated Core Foundation calls and unnecessary unsafe blocks against the locked bindings); registry compilation had capped those dependency diagnostics. Application Clippy remains warnings-denied. No warning suppression or macOS API migration is part of this ownership patch.

### GTK filename and list ownership

The REQ-ENG-02/PERF-01 follow-up releases the single returned filename after copying it, releases every multiple-selection filename and the original list head after collection, and releases the message-area child-list links while retaining borrowed widgets. Missing filenames return no path; invalid UTF-8 paths retain 0.17.2's filtering. The production changes stay within the two GTK backend files; private Linux test hooks also require one structural module declaration. No dependency versions, features or lockfile bytes changed.

The integrity verifier now checks nine disjoint hunks across three files against all 61 published file hashes. Offline and authenticated-archive checks passed, as did all 18 verifier tests. The Linux ownership fixture calls the real patched functions through private test-only FFI bindings, checks copy-before-release and exactly-once cleanup, and uses dependency artifacts from the locked root desktop build. Its six getter cases and borrowed-child case run without a display or GTK initialization. At `65042d3`, Linux x64 and arm64 each compiled the real patched functions against the locked desktop graph and passed both named ownership tests: six getter cases and the borrowed-child case. Their installed Xvfb checks also passed separately. These tests establish the stated allocation invariants; they do not measure process memory or establish future-cancellation cleanup. [Reproduction and test scope](../../crates/vendor/verification/rfd/README.md)

The first hosted attempt at `747d1b2` failed both Linux ownership-test compilations with E0463 before executing any cases. The runner provided direct library paths but omitted transitive libraries' separate Cargo 1.100 build-unit directories. The correction validates every root-build `.rlib` and supplies their parent directories while preserving the four exact direct externs. A local display-free root-CLI probe reproduced the missing-metadata error and passed with the complete reported paths. The installed Xvfb checks were skipped after the failed gate; they are not counted as runtime evidence for that attempt. [Failed run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46097806657)

## Native Linux clipboard

At `d180c46`, the desktop crate directly uses the already locked `gtk` 0.18.2 package for exact prepared-bank clipboard text. The dependency is optional and Linux-only, enabled by the desktop feature. Locked metadata declares MIT and Rust 1.70; Rust 1.100.0 remains the workspace pin. The lockfile change adds only `gtk` to the desktop dependency list, without adding packages or changing versions.

The local main webview can request a context-free, revision-checked engine preparation. The native command checks signature consent, decodes through `sc2bank` and publishes owned raw UTF-8 data on GTK's main thread. It waits for the ownership claim before returning success. The product command does not read the clipboard. It advertises `UTF8_STRING` and `text/plain;charset=utf-8`; the independent installed X11 test requests only `UTF8_STRING`. Other desktop platforms retain the WebView path. Runtime qualification and its remaining limits are recorded with the [clipboard evidence](IMPLEMENTATION.md#linux-clipboard-text-preservation).

## Linux accessibility test tools

The installed Linux accessibility fixture adds Noble's test-only `at-spi2-core` `2.52.0-1build1` on x64 and arm64. The upstream code uses LGPL-2.1-or-later. The existing GTK development dependencies provide `libatspi2.0-dev`; the probe uses the native AT-SPI enum headers and links GIO for direct typed D-Bus calls. `dbus-run-session` comes from the runner's D-Bus package and creates a private session inside Xvfb. The workflow records the installed package versions. These tools are outside the application bundle. [Ubuntu package](https://packages.ubuntu.com/noble/at-spi2-core) · [Bridge source](https://github.com/GNOME/at-spi2-core/tree/AT_SPI2_CORE_2_52_0) · [D-Bus session lifecycle](https://dbus.freedesktop.org/doc/dbus-run-session.1.html)

The fixture clears inherited accessibility-bus and Wayland overrides, activates the private accessibility bus through `GetAddress`, then queries the exact app frame and WebView document. It requires the focused editable `ENTRY` name, line mode and complete native Text value for Table and Raw. It does not set `org.a11y.Status.IsEnabled`, whose implementation can persist GSettings. Owned GIO connections, GVariant replies and strings are released, and traversal/RPC/process budgets bound the probe. [WebKitGTK states](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebCore/accessibility/atspi/AccessibilityObjectAtspi.cpp#L708-L736) · [Native Text interface](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebCore/accessibility/atspi/AccessibilityObjectTextAtspi.cpp#L259-L330) · [Probe](../../apps/starbank-desktop/verification/linux-accessibility.c)

Local C11 syntax checking against the exact Noble AT-SPI and D-Bus headers passed without linking or launching a GUI. Pinned Node syntax, Biome, the five strict window-observer tests and Actionlint passed. At `df4c7b7`, both Linux builds linked the probe and activated the private accessibility bus, but the first Table query reached the three-second process watchdog before producing a native result. The run finished 9/11; the 12 simulated accounting cases, both GTK ownership and strict window-observer gates, and package cleanup passed. No Table or Raw accessibility success or subsequent installed workflow is inferred from this attempt. The diagnostic run at `adb947b` located the stall in `atspi_accessible_get_child_count` on both architectures. The direct client retains the same assertions and budgets while using typed GDBus calls to the native Accessible and Text interfaces; it avoids libatspi's reentrant client dispatch path. At `0ace09f`, both installed Linux builds verified the native Table control's role, focused/editable states and exact `001` text. Raw passed its native role/state/interface checks but failed its character-count comparison; its text result and subsequent installed flows remain unverified. The diagnostic run at `5bacfb0` observed the complete native Raw text on both architectures: 100 characters for the 99-character XML, with exactly one trailing LF and no truncation. WebKit uses rendered `innerText` for this nonnative text control, and its text iterator emits the CodeMirror block boundary. The fixture now expects precisely that presentation LF while its separate DOM/model/source-byte assertions still require the original XML without it. The probe continues to compare the complete native count and text exactly; it does not trim or normalize the reply. [Raw native text observation](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46105345159) · [WebKit text iterator](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebCore/editing/TextIterator.cpp#L1046-L1078) [Direct-client run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46104777884) [Call-stage run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46103872562) [First accessibility run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46102969694) At `80e45aa`, all eleven hosted jobs passed. Both installed Linux architectures verified the focused Table and Raw entries through native AT-SPI, then completed all three existing UI sessions, exact clipboard checks, 114 desktop tests and package cleanup. The complete native Raw comparison includes the observed presentation LF; source and engine XML assertions retain their original bytes. This qualifies the two controls' accessibility-tree exposure; screen-reader speech and user navigation remain separate acceptance cases. [Qualified native accessibility run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46105717850)

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

The Linux native WebView CI check installs test-only `tauri-driver` 2.0.6 with `cargo install --version '=2.0.6' --locked` into a runner-temporary prefix. The published crate declares Rust 1.77.2 and `Apache-2.0 OR MIT`, and its source archive includes `Cargo.lock`. Its verified registry SHA-256 is `24dc39bd26a65361c1c8e067636bbff1d9dd7e2fc58ff874edfba33acab0e6d0`. Ubuntu 24.04 supplies `webkit2gtk-driver` and `xvfb` for x64 and arm64; Xvfb depends on `xauth`. The Node harness uses built-in APIs. This adds no application dependency, plugin, workspace lockfile change or test-only application IPC. The basic native workflow passed on both architectures at `0536fbc`. [Pinned driver](https://github.com/tauri-apps/tauri/tree/tauri-driver-v2.0.6/crates/tauri-driver) · [Registry package](https://static.crates.io/crates/tauri-driver/tauri-driver-2.0.6.crate) · [Ubuntu driver](https://packages.ubuntu.com/noble-updates/webkit2gtk-driver) · [Xvfb dependencies](https://packages.ubuntu.com/noble/xvfb) · [Runtime evidence](IMPLEMENTATION.md#headless-linux-native-webview-qualification)

The GTK chooser slice uses Ubuntu Noble's test-only `xdotool` (`1:3.20160805.1-5build1`, x64 and arm64; upstream BSD-3-Clause). Bounded shell-free calls send XTEST keyboard events only after verifying the exact chooser title and X focus on the isolated Xvfb display. It adds no runtime application dependency. The chooser check passed on both architectures at `5f93fc1`. [Ubuntu package](https://packages.ubuntu.com/noble/xdotool) · [Upstream commands](https://github.com/jordansissel/xdotool/blob/v3.20160805.1/xdotool.pod) · [License](https://github.com/jordansissel/xdotool/blob/v3.20160805.1/COPYRIGHT) · [Runtime evidence](IMPLEMENTATION.md#headless-linux-native-webview-qualification)

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

The backport root Cargo.lock SHA-256 is `a8f753116d1aea9945b5c5e7c7ba405468b233468e33f78b5ff7f742a7639258`. The published version set is unchanged. At `912ea6d`, hosted Windows passed 109 desktop tests and silent NSIS verification; macOS passed 123 tests and unsigned app verification with the recorded publisher warnings.
