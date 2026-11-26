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
- [ ] Integrate CI and the frontend bootstrap. Qualify remaining production dependency selections at their owning boundaries.
- [ ] Implement and verify: REQ-ENG-01, REQ-ENG-02, REQ-SCOPE-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-03 — Lossless format engine

Depends on: 02.

- [ ] Parser and writer, real fixture corpus, unknown handling, bounds and fuzz harness.
- [ ] Implement and verify: REQ-FMT-01, REQ-FMT-02, REQ-FMT-03, REQ-FMT-04.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-04 — Signature and context

Depends on: 03.

- [ ] Canonical vectors and differential oracle; verify before export.
- [ ] Implement and verify: REQ-SIG-01, REQ-SIG-02, REQ-SIG-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-05 — Editor transactions and validation

Depends on: 03,04.

- [ ] Stable IDs, numeric lexemes, bulk transaction, raw draft, undo/reset/diff and conflict protocol.
- [ ] Implement and verify: REQ-EDIT-01, REQ-EDIT-02, REQ-EDIT-03, REQ-EDIT-04, REQ-EDIT-05, REQ-EDIT-06, REQ-EDIT-07.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-06 — Game profiles

Depends on: 05.

- [ ] Versioned schema, migration, fixture-backed Simple View mapping and dependent validation.
- [ ] Implement and verify: REQ-PROF-01, REQ-PROF-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-07 — Safe persistence and backup

Depends on: 04,05.

- [ ] Verified prepare/commit, Windows/macOS tests, explicit outcomes, backup restore; license-safe maintained library selection.
- [ ] Implement and verify: REQ-SAVE-01, REQ-SAVE-02, REQ-BACKUP-01, REQ-BACKUP-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-08 — Native paths and bank inventory

Depends on: 02.

- [ ] First valid root, all banks, Linux stub/manual, scopes and resolver fixtures.
- [ ] Implement and verify: REQ-PATH-01, REQ-PATH-02, REQ-MAP-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-09 — MPQ feasibility and adapter

Depends on: 02.

- [ ] Evaluate candidates against SC2/StormLib, native read/write capability, no runtime tampering; unsupported map tool remains explicit.
- [ ] Implement and verify: REQ-MPQ-01, REQ-MPQ-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-10 — Map indexing and cache

Depends on: 08,09.

- [ ] Incremental metadata, real grouping, bounded workers and cancellation, map filters/status.
- [ ] Implement and verify: REQ-MAP-02, REQ-MAP-03.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-11 — Shared UI shell

Depends on: 02,08.

- [ ] React/HeroUI design at 850×500; pinned scroll, menus, metadata and quick options; synthetic fixtures only.
- [ ] Implement and verify: REQ-UI-01, REQ-UI-02, REQ-UI-03, REQ-UI-04.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-12 — Live editor UI

Depends on: 05,06,07,10,11.

- [ ] Bind all views to engine, exact formatted inputs, raw CodeMirror, same-value bulk, backup and save controls.
- [ ] Implement and verify: REQ-UI-05, REQ-UI-06, REQ-UI-07, REQ-UI-08.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-13 — Settings and platform integration

Depends on: 11,12.

- [ ] Settings page, legacy migration, capability-aware native tools/permissions, safe switching.
- [ ] Implement and verify: REQ-UI-09.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-14 — StarBank Web

Depends on: 03,04,05,06,11,12.

- [ ] WASM worker, browser adapter, local-only static site; no need for native discovery/MPQ.
- [ ] Implement and verify: REQ-WEB-01, REQ-WEB-02.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-15 — CLI and agent skill

Depends on: 05,07,08.

- [ ] Typed commands, JSON, dry-run, no implicit overwrite; docs and conformance.
- [ ] Implement and verify: REQ-CLI-01.
- [ ] Add crate-local tests/appropriate verification, run applicable gates, record commands, unresolved issues and atomic Conventional Commit(s).

## WP-16 — Distribution and associations

Depends on: 12,13,14,15.

- [ ] Real Windows/macOS install/open flow; release licensing cleared; Linux support documented.
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
