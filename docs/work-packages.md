# Implementation work packages

Read AGENTS and EES/EVS and record evidence per work package. Parallelizable after WP-02: format, resolver, archive spike, writer spike and UI shell in separate worktrees. Shared schemas and manifests belong to the integrator. Release parity requires every Required row or an explicit owner-approved exception.

## WP-01 — Archive and provenance

Depends on: none.

- [x] Reconcile the live inventory and manifest, verify the archived bytes and record the license blocker.
- [x] Verify REQ-MIG-01 with the archive move in `4119421` and the checks in [GO_INVENTORY.md](reference/GO_INVENTORY.md).
- [ ] Complete REQ-MIG-02 provenance review and obtain owner approval for the release license before publication.

## WP-02 — Workspace and risk spikes

Depends on: 01.

- [x] Verify the pinned toolchain, establish the Rust workspace and record the shared DTO contract and initial risk spikes. [Bootstrap evidence](verification/BOOTSTRAP.md)
- [x] Integrate Rust and frontend CI, the local source gate and the frontend bootstrap.
- [x] Pass the hosted source, wasm32 and frontend checks and the Linux and macOS jobs at `4143952`. [Hosted evidence](verification/IMPLEMENTATION.md#hosted-ci-at-4143952)
- [x] Run the pinned local Rust and frontend advisory checks and collect installed frontend dependency license inventories. [Dependency checks](verification/DEPENDENCIES.md)
- [x] Pass all 11 hosted CI jobs at `e4385da`, including the Linux arm64 desktop leg, both advisory checks and the previously failing Windows probes. [Hosted evidence](verification/IMPLEMENTATION.md#hosted-ci-at-e4385da)
- [x] Fix the pinned RFD GTK native-chooser reference leak and qualify repeated async picker teardown. [Checked backport](verification/DEPENDENCIES.md#gtk-native-chooser-reference-backport)
- [x] Release the separate GTK filename/list allocations while preserving existing UTF-8 path behavior; verify real-function ownership tests and installed dialog regressions on Linux x64 and arm64. [Ownership evidence](verification/DEPENDENCIES.md#gtk-filename-and-list-ownership)
- [ ] Complete production dependency and capability qualification for REQ-ENG-01, REQ-ENG-02 and REQ-SCOPE-01. Verify later revisions in their own hosted runs.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-03 — Lossless format engine

Depends on: 02.

- [x] Implement bounded UTF-8 and BOM-marked UTF-16 parsing, lossless export, and native-byte-preserving value, structural and signature edits. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Check in and run the bounded lossless-format fuzz target. [Fuzz harness](../crates/formats/sc2bank/fuzz/README.md)
- [x] Run the structural-edit fuzz target across UTF-8 and both supported UTF-16 byte orders. [Fuzz harness](../crates/formats/sc2bank/fuzz/README.md)
- [ ] Qualify real SC2 inputs, confirm the observed encoding scope, and run sustained fuzzing for REQ-FMT-01, REQ-FMT-02, REQ-FMT-03 and REQ-FMT-04.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-04 — Signature and context

Depends on: 03.

- [x] Implement signature computation, verification and lossless in-memory re-signing for the intended destination context. Verify independent synthetic digests and native/WASM/CLI agreement. [Adapter parity](verification/ADAPTER_PARITY.md)
- [x] Wire automatic and explicit re-signing, including create-only signed copies of external banks. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Verify the macOS native external-signing picker workflow on synthetic banks. [macOS evidence](verification/MACOS_EXTERNAL_SIGNING.md)
- [x] Verify the installed Linux x64 and arm64 synthetic external-signing case through real GTK choosers: independently expected 170-byte output, unchanged sources, retained dirty edit/Undo, settings opt-out and post-close file checks. All eleven CI jobs passed at `e739ed0`; intermittent chooser failures remain open below. [Linux evidence](verification/LINUX_EXTERNAL_SIGNING.md)
- [x] Verify one installed Linux malformed-author rejection and keyboard repair before signed-copy creation, preserving the active edit and source files. All eleven jobs passed at `da404e3`. [Rejection evidence](verification/LINUX_EXTERNAL_SIGNING.md#malformed-context-and-repair)
- [x] Verify installed Linux signing-form Cancel discards entered context, preserves the dirty editor and sources, and reopens an empty form before signing. All eleven jobs passed at `3208117`. [Cancellation evidence](verification/LINUX_EXTERNAL_SIGNING.md#cancellation-and-reopen)
- [ ] Resolve the intermittent GTK source-chooser failure. The mapped-entry diagnostic and its same-code repeat passed all eleven jobs each; those guarded sessions do not establish a fix for ordinary key timing. [Diagnostic scope](verification/LINUX_EXTERNAL_SIGNING.md#mapped-entry-diagnostic-follow-up)
- [x] Match the stored signatures of two owner-supplied banks against independently calculated digests and native/WASM/CLI output. [Private corpus](verification/PRIVATE_SC2_CORPUS.md)
- [ ] Verify documented SC2-generated digests across a broader corpus and game acceptance of edited copies for REQ-SIG-01, REQ-SIG-02 and REQ-SIG-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-05 — Editor transactions and validation

Depends on: 03,04.

- [x] Implement stable IDs, exact numeric lexemes, atomic edits, structural edits, validated Raw replacement, history and opening-snapshot diff/revert in the shared engine.
- [x] Check structured value drafts without changing the document, using the same Rust normalization as committed edits. [Validation tests](../crates/domain/starbank-editor/tests/value_validation.rs)
- [ ] Implement and verify: REQ-EDIT-01, REQ-EDIT-02, REQ-EDIT-03, REQ-EDIT-04, REQ-EDIT-05, REQ-EDIT-06, REQ-EDIT-07.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-06 — Game profiles

Depends on: 05.

- [x] Implement versioned schema, compatible legacy migration, explicit identity matching and exact atomic dependent edits in the engine.
- [x] Wire Simple View and verify synthetic profile selection, dependent changes and one-step undo. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Allow first legacy YAML import to request a scoped manual-author retry; verify stale-scope refusal and unchanged bank bytes/revision. [Import evidence](verification/IMPLEMENTATION.md#first-legacy-profile-import-without-inferred-author)
- [ ] Qualify actual game profiles and their identity evidence.
- [ ] Implement and verify: REQ-PROF-01, REQ-PROF-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-07 — Safe persistence and backup

Depends on: 04,05.

- [x] Implement checked preparation, exclusive creation, verified local APFS replacement, backup snapshots and reversible restore policy.
- [x] Verify the APFS save, backup and restore paths with a synthetic bank. Exercise process termination and retained bytes around replacement. [Native storage evidence](../crates/platform/starbank-fs/verification/NATIVE_STORAGE.md)
- [x] Exercise staging space exhaustion on disposable APFS and Linux tmpfs fixtures. Verify refusal before publication and preservation of existing bank bytes. [Storage fault evidence](../crates/platform/starbank-fs/verification/NATIVE_STORAGE.md)
- [x] Verify sibling and distinct-root create-only rename and collision refusal in the hosted Windows local NTFS probe. [Probe evidence](../crates/platform/starbank-fs/verification/WINDOWS_CREATE_QUALIFICATION.md#hosted-ntfs-probe)
- [x] Qualify the Windows local NTFS create-only transaction under a reduced token, including collisions, process interruption, real staging disk-full rejection and explicit verified retry. [Windows create evidence](../crates/platform/starbank-fs/verification/WINDOWS_CREATE_QUALIFICATION.md)
- [x] Verify Windows public CLI copy/adjacent-backup writes and native Save As, export and external-signing callbacks without launching dialogs. [Windows integration evidence](../crates/platform/starbank-fs/verification/WINDOWS_CREATE_QUALIFICATION.md#qualified-transaction-and-create-only-activation)
- [x] Verify installed Linux x64/arm64 real GTK Export Copy and Save As before queued navigation, with exact outputs and unchanged source banks. [Native chooser evidence](verification/IMPLEMENTATION.md#native-unsaved-dialog-response-compatibility)
- [x] Offer Windows create-only Save As before navigation/close with verified-checkpoint guards, native filesystem tests and Windows-labelled renderer checks. Actual Windows dialogs remain unverified. [Guarded Save As](verification/IMPLEMENTATION.md#windows-save-as-before-navigation-or-close)
- [ ] Qualify Windows replacement separately, Linux replacement with hidden metadata and physical power-loss recovery. [Native replacement evidence](../crates/platform/starbank-fs/verification/NATIVE_REPLACEMENT_QUALIFICATION.md)
- [ ] Implement and verify: REQ-SAVE-01, REQ-SAVE-02, REQ-BACKUP-01, REQ-BACKUP-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-08 — Native paths and bank inventory

Depends on: 02.

- [x] Implement prioritized candidates, Linux manual mode, scoped inventory and signing hints. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Verify installed Linux folder Cancel, nested loose-collection import and explicit Refresh, retaining the dirty editor and exact source bytes. All eleven jobs passed at `f67a74d`. [Manual collection evidence](verification/IMPLEMENTATION.md#installed-linux-manual-collection)
- [x] Verify installed Linux manual-collection persistence across a fresh process restart with shared config only: three restored banks, no automatic open, and a known bank opened clean with unchanged source bytes. All eleven jobs passed at `f2c42dd`. [Restart evidence](verification/IMPLEMENTATION.md#installed-linux-manual-collection-restart)
- [x] Verify installed Linux missing manual-root recovery after a synthetic directory rename: retain the explicit failure without automatic fallback, select the relocated three-bank collection through GTK, and preserve all bytes without opening a bank. All eleven jobs passed at `cdd0e26`. [Recovery evidence](verification/IMPLEMENTATION.md#installed-linux-missing-manual-collection-recovery)
- [ ] Verify real platform paths, player display names and UI integration for REQ-PATH-01, REQ-PATH-02 and REQ-MAP-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-09 — MPQ feasibility and adapter

Depends on: 02.

- [x] Implement bounded read-only metadata inspection for supported MPQ V1/V2/V4 layouts and test synthetic StormLib fixtures.
- [x] Exercise verified archival copies and explicit capability fallback with synthetic MPQ fixtures. [Copy qualification](../crates/adapters/starbank-mpq/verification/MAP_EXPORT.md)
- [x] Verify add-only classic V1 output against original hash slots, block records and packed bytes. [Verifier tests](../crates/adapters/starbank-mpq/tests/v1_append.rs)
- [x] Exercise opt-in macOS arm64 StormLib append and fuzz the pure verifier on synthetic fixtures. [Native append](../crates/adapters/starbank-mpq-stormlib/verification/NATIVE_APPEND.md) · [Fuzz run](../crates/adapters/starbank-mpq/fuzz/README.md)
- [x] Compare four private local cache copies with StormLib, including declared origins and padded sector tables: eight exact member matches and four agreed absences. Capture provenance remains unknown. [Local cache evidence](verification/PRIVATE_SC2_CORPUS.md#existing-local-cache-snapshots)
- [ ] Qualify authorized SC2 archives, exact original components and SC2 Editor repair and reopen.
- [ ] Implement and verify: REQ-MPQ-01, REQ-MPQ-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-10 — Map indexing and cache

Depends on: 08,09.

- [x] Implement bounded and cancellable metadata indexing, incremental cache and ambiguity-preserving association. Verify synthetic archives and cache failures.
- [x] Connect map selection, filters, metadata, progress and generation-scoped cancellation to the native application. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Verify unchanged-cache reuse on the four private copies in a headless macOS scan: identical records, four warm hits and unchanged inputs. [Cache observation](verification/PERFORMANCE_BASELINE.md#four-private-cache-copies)
- [ ] Test real cache layouts and live cancellation across platforms.
- [ ] Implement and verify: REQ-MAP-02, REQ-MAP-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-11 — Shared UI shell

Depends on: 02,08.

- [x] Implement and browser-check the resizable map list, search, filters, selection and Quick Options using synthetic inventory. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Wire discovery, settings, breadcrumbs, menus, map metadata and bank choice. Verify responsive behavior and editor shortcuts. [Implementation evidence](verification/IMPLEMENTATION.md)
- [ ] Verify the remaining native visual and accessibility cases for REQ-UI-01, REQ-UI-02, REQ-UI-03 and REQ-UI-04.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-12 — Live editor UI

Depends on: 05,06,07,10,11.

- [x] Implement controlled Table and CodeMirror Raw views, same-value bulk editing, draft safeguards and callback-driven file actions.
- [x] Bind live Table, Raw, and Simple View and qualify the supported local save, backup and restore flows with synthetic banks. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Preserve numeric pointer selections when entering exact values and provide bounded, advisory Raw syntax hints. Verify both in all three headless browser engines. [Editor evidence](verification/IMPLEMENTATION.md#req-ui-06-editor-input-follow-ups)
- [x] Verify installed Linux Raw keyboard editing, native rejection, repair, Apply and Undo while preserving queued files and source bytes. [Raw evidence](verification/IMPLEMENTATION.md#installed-linux-raw-editing)
- [x] Verify installed Linux unapplied-draft close protection, cancelled/verified Save As on close and independent exact X11 clipboard readback under Xvfb. [Native evidence](verification/IMPLEMENTATION.md#native-unsaved-dialog-response-compatibility)
- [x] Verify installed Linux Table/Raw native accessibility roles, names, focus, editability, line modes and exact text on x64 and arm64. [Native accessibility](verification/DEPENDENCIES.md#linux-accessibility-test-tools)
- [x] Verify installed Linux Save menu order, keyboard navigation, Escape focus return and outside dismissal. [Native menu evidence](verification/IMPLEMENTATION.md#linux-clipboard-text-preservation)
- [x] Verify installed Linux Tools menu order, disabled-item skipping, Escape trigger focus and outside dismissal retaining Table tab focus, without activating actions. [Native Tools evidence](verification/IMPLEMENTATION.md#linux-clipboard-text-preservation)
- [x] Verify exact X11 `UTF8_STRING` and `text/plain;charset=utf-8` clipboard text from an edited UTF-16LE bank with BOM, CRLF and Unicode. [Clipboard evidence](verification/IMPLEMENTATION.md#linux-clipboard-text-preservation)
- [ ] Complete the installed native platform and assistive technology matrix. Keep writes scoped to qualified storage.
- [ ] Complete installed native and assistive-technology acceptance for REQ-UI-05, REQ-UI-06, REQ-UI-07 and REQ-UI-08.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-13 — Settings and platform integration

Depends on: 11,12.

- [x] Implement versioned browser preferences, legacy decoding and the shared settings page. Preserve in-flight drafts across settings navigation.
- [x] Persist native preferences and authorized roots. Share account selection, appearance, automatic backup policy and platform capability information in Settings. Add explicit map-index rebuild. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Configure a managed backup folder and persist a retention count with a read-only review of older backups. Keep access to earlier managed folders. [Implementation evidence](verification/IMPLEMENTATION.md)
- [x] Verify the installed Linux auto re-sign preference survives a fresh process restart and controls exact unsigned Save As output. [Restart evidence](verification/IMPLEMENTATION.md#installed-linux-preference-restart)
- [ ] Complete installed native picker and platform acceptance for REQ-UI-09.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-14 — StarBank Web

Depends on: 03,04,05,06,11,12.

- [x] Wire the browser to the shared WASM engine and verify offline local editing, guarded switching, copy fallback and downloads in Chromium, Firefox and WebKit.
- [x] Verify live profile and structural editing controls and exact opening backups across the supported browser engines. [Browser write qualification](verification/BROWSER_WRITES.md)
- [x] Ship static editor and worker response headers and verify local static hosting at a nested path in the browser matrix. [Implementation evidence](verification/IMPLEMENTATION.md)
- [ ] Verify the static hosted build. Reassess writable handles if browsers expose the needed conflict and recovery guarantees.
- [ ] Qualify the hosted static deployment for REQ-WEB-01 and REQ-WEB-02; preserve the recorded three-engine local evidence.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-15 — CLI and agent skill

Depends on: 05,07,08.

- [x] Implement typed local bank commands, versioned JSON, exact patch selectors, dry-run, guarded effects, backup and restore, and the CLI skill.
- [x] Implement read-only map inspection and qualify the local CLI on macOS and Linux. [CLI guide](../apps/starbank-cli/README.md)
- [x] Compare native protocol, release WASM, and CLI output on the same three synthetic encodings, including exact signing and dry-run behavior. [Adapter parity](verification/ADAPTER_PARITY.md)
- [x] Verify Windows CLI reads and no-write preparation on synthetic UTF-8, UTF-16LE and UTF-16BE banks. [Hosted qualification](verification/IMPLEMENTATION.md#headless-follow-up-qualification)
- [ ] Complete cross-adapter conformance on authorized game data.
- [ ] Implement and verify: REQ-CLI-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-16 — Distribution and associations

Depends on: 12,13,14,15.

- [x] Build and launch the Tauri host on macOS and link the tested Linux arm64 executable.
- [x] Verify the local unsigned macOS disk image and the Debian arm64 package and its install, launch and purge flow. [Distribution evidence](verification/DISTRIBUTION.md)
- [x] Verify the unsigned Windows x64 NSIS installer, per-user Open With registration, and silent uninstall with a synthetic bank. [Hosted installer evidence](verification/DISTRIBUTION.md#windows-x64-nsis)
- [x] Build and inspect unsigned macOS arm64 and Linux x64/arm64 packages in CI; verify four synthetic Windows settings/backup/recovery sentinels survive silent install/uninstall. [Package qualification](verification/DISTRIBUTION.md#continuous-headless-package-qualification)
- [x] Install the verified Noble x64/arm64 Debian packages, run the real editor and GTK dialogs headlessly from the installed binary, and verify owned purge. [Installed Linux qualification](verification/DISTRIBUTION.md#continuous-headless-package-qualification)
- [x] Qualify the dormant Windows running-instance transport with five headless native probes. [Windows transport evidence](../crates/platform/starbank-instance/verification/WINDOWS_INSTANCE.md#hosted-native-qualification)
- [x] Wire Windows launch delivery into the guarded app queue and verify it with a real pipe and a windowless Tauri runtime. [Native app integration](../crates/platform/starbank-instance/verification/WINDOWS_INSTANCE.md#desktop-integration)
- [ ] Verify signed packages, Windows Explorer Open With visibility and open flows, installed macOS open flows, and release licensing.
- [ ] Implement and verify: REQ-DIST-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-17 — Verification and performance gate

Depends on: 03–16.

- [x] Record a repeatable Chromium startup and interaction baseline and run the bounded XML sanitizer and Miri checks. [Browser timing](verification/BROWSER_PERFORMANCE.md) · [Miri evidence](../crates/formats/sc2bank/verification/MIRI.md)
- [x] Measure macOS app-host startup and resources, and check the bounded save-and-restore model. [macOS timing](../apps/starbank-desktop/verification/native-performance/NATIVE_PERFORMANCE.md) · [Save and backup model](../crates/platform/starbank-fs/verification/save-backup-model/README.md)
- [ ] Ongoing from first WP; report actual tests, fault injection, corpus parity, measured budgets and formal coverage.
- [ ] Implement and verify: REQ-PERF-01, REQ-QA-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-18 — Deferred scope

Depends on: core release.

- [ ] Only with explicit priority/approval; do not block initial product.
- [ ] Implement and verify: REQ-LATER-01, REQ-LATER-02, REQ-LATER-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).
