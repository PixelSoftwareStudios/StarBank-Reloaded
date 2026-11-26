# Implementation work packages

Read AGENTS and EES/EVS and record evidence per work package. Parallelizable after WP-02: format, resolver, archive spike, writer spike and UI shell in separate worktrees. Shared schemas and manifests belong to the integrator. Release parity requires every Required row or an explicit owner-approved exception.

## WP-01 — Archive and provenance

Depends on: none.

- [x] Reconcile the live inventory and manifest, verify the archived bytes and record the license blocker.
- [x] Verify REQ-MIG-01 with the archive move in `4119421` and the checks in [GO_INVENTORY.md](docs/reference/GO_INVENTORY.md).
- [ ] Complete REQ-MIG-02 provenance review and obtain owner approval for the release license before publication.

## WP-02 — Workspace and risk spikes

Depends on: 01.

- [x] Verify the pinned toolchain, establish the Rust workspace and record the shared DTO contract and initial risk spikes. [Bootstrap evidence](docs/verification/BOOTSTRAP.md)
- [x] Integrate Rust and frontend CI, the local source gate and the frontend bootstrap.
- [ ] Verify hosted CI and production dependency and capability selections for REQ-ENG-01, REQ-ENG-02 and REQ-SCOPE-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-03 — Lossless format engine

Depends on: 02.

- [x] Implement bounded UTF-8 parsing, lossless unchanged export and targeted existing-value edits. [Implementation evidence](docs/verification/IMPLEMENTATION.md)
- [ ] Qualify real SC2 inputs and other encodings, complete fuzz coverage and verify the remaining acceptance cases for REQ-FMT-01, REQ-FMT-02, REQ-FMT-03 and REQ-FMT-04.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-04 — Signature and context

Depends on: 03.

- [x] Implement signature computation, verification and lossless in-memory re-signing for the intended destination context. Compare synthetic vectors with the pinned v2.0 signer.
- [ ] Verify game-generated digests and complete automatic and explicit re-sign flows for REQ-SIG-01, REQ-SIG-02 and REQ-SIG-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-05 — Editor transactions and validation

Depends on: 03,04.

- [x] Implement stable IDs, exact numeric lexemes, atomic edits, structural edits, validated Raw replacement, history and opening-snapshot diff/revert in the shared engine.
- [ ] Implement and verify: REQ-EDIT-01, REQ-EDIT-02, REQ-EDIT-03, REQ-EDIT-04, REQ-EDIT-05, REQ-EDIT-06, REQ-EDIT-07.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-06 — Game profiles

Depends on: 05.

- [x] Implement versioned schema, compatible legacy migration, explicit identity matching and exact atomic dependent edits in the engine.
- [ ] Wire Simple View and qualify actual game profiles and identity evidence.
- [ ] Implement and verify: REQ-PROF-01, REQ-PROF-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-07 — Safe persistence and backup

Depends on: 04,05.

- [x] Implement checked preparation, exclusive creation, verified local APFS replacement, backup snapshots and reversible restore policy.
- [ ] Qualify Windows writes, Linux replacement, interrupted-save recovery and the full application save and backup flow.
- [ ] Implement and verify: REQ-SAVE-01, REQ-SAVE-02, REQ-BACKUP-01, REQ-BACKUP-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-08 — Native paths and bank inventory

Depends on: 02.

- [x] Implement prioritized candidates, Linux manual mode, scoped inventory and signing hints. [Implementation evidence](docs/verification/IMPLEMENTATION.md)
- [ ] Verify real platform paths, player display names and UI integration for REQ-PATH-01, REQ-PATH-02 and REQ-MAP-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-09 — MPQ feasibility and adapter

Depends on: 02.

- [x] Implement bounded read-only metadata inspection for supported MPQ V1/V2/V4 layouts and test synthetic StormLib fixtures.
- [ ] Qualify the real SC2 corpus and authorized repair and export behavior.
- [ ] Implement and verify: REQ-MPQ-01, REQ-MPQ-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-10 — Map indexing and cache

Depends on: 08,09.

- [x] Implement bounded and cancellable metadata indexing, incremental cache and ambiguity-preserving association. Verify synthetic archives and cache failures.
- [ ] Connect map selection, filters and metadata status to the native application and test real cache layouts.
- [ ] Implement and verify: REQ-MAP-02, REQ-MAP-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-11 — Shared UI shell

Depends on: 02,08.

- [x] Implement and browser-check the resizable map list, search, filters, selection and Quick Options using synthetic inventory. [Implementation evidence](docs/verification/IMPLEMENTATION.md)
- [ ] Wire discovery and settings. Complete breadcrumbs, menus, map metadata and bank choice for REQ-UI-01, REQ-UI-02, REQ-UI-03 and REQ-UI-04.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-12 — Live editor UI

Depends on: 05,06,07,10,11.

- [x] Implement controlled Table and CodeMirror Raw views, same-value bulk editing, draft safeguards and callback-driven file actions.
- [ ] Finish all live platform bindings, Simple View, and backup and save flows.
- [ ] Implement and verify: REQ-UI-05, REQ-UI-06, REQ-UI-07, REQ-UI-08.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-13 — Settings and platform integration

Depends on: 11,12.

- [x] Implement versioned browser preferences, legacy decoding and the shared settings page. Preserve in-flight drafts across settings navigation.
- [ ] Complete native preference persistence, root authorization and remaining platform settings.
- [ ] Implement and verify: REQ-UI-09.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-14 — StarBank Web

Depends on: 03,04,05,06,11,12.

- [x] Wire the browser to the shared WASM engine and verify offline local editing, guarded switching, copy fallback and downloads in Chromium, Firefox and WebKit.
- [ ] Complete live profile and structural editing controls, optional writable file handles, and static deployment verification.
- [ ] Implement and verify: REQ-WEB-01, REQ-WEB-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-15 — CLI and agent skill

Depends on: 05,07,08.

- [x] Implement typed local bank commands, versioned JSON, exact patch selectors, dry-run, guarded effects, backup and restore, and the CLI skill.
- [ ] Finish map inspection and cross-adapter and native Windows runtime conformance.
- [ ] Implement and verify: REQ-CLI-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-16 — Distribution and associations

Depends on: 12,13,14,15.

- [x] Build and launch the Tauri host on macOS and link the tested Linux arm64 executable.
- [ ] Verify signed packages and file associations, real Windows and macOS install/open flows, release licensing and Linux packaging.
- [ ] Implement and verify: REQ-DIST-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-17 — Verification and performance gate

Depends on: 03–16.

- [ ] Ongoing from first WP; report actual tests, fault injection, corpus parity, measured budgets and formal coverage.
- [ ] Implement and verify: REQ-PERF-01, REQ-QA-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-18 — Deferred scope

Depends on: core release.

- [ ] Only with explicit priority/approval; do not block initial product.
- [ ] Implement and verify: REQ-LATER-01, REQ-LATER-02, REQ-LATER-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).
