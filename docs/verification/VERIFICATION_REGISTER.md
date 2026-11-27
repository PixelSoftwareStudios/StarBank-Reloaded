# Verification obligations

All Pending unless stated otherwise. Evidence is crate-local; these rows track intent and gate, not completed proofs. Add reproducible harness path, revision, tool version, constraints and results during implementation.

| Obligation | Requirements | Method | Invariant / gate |
|---|---|---|---|
| VO-01 | REQ-FMT-01/02/03/04 | rstest, proptest, insta, fuzz, Miri | Lossless unchanged bytes, preservation or rejection on edit, safe limits; no panic on malformed input |
| VO-02 | REQ-SIG-01/02/03 | Differential C# and real game corpus, synthetic digest vectors, property tests, optional Lean/Kani | Exact order/encoding/context, no mutation of source order; validate before sign; no proof of SHA1 security |
| VO-03 | REQ-EDIT-01/02/03/04/05/06/07 | Proptest, Kani bounded state/number operations, Lean refinement feasibility, Loom/Shuttle revision ordering | Stable IDs, precise canonical numeric strings, atomic multi-edit, reset/undo and stale command rejection |
| VO-04 | REQ-PROF-01/02 | Proptest, fuzz, Kani checked rules | All-or-none dependent graph, match confidence, cycles/overflow rejected |
| VO-05 | REQ-SAVE-01/02, REQ-BACKUP-01/02 | TLA+/TLC commit states, Loom/Shuttle ownership, failpoints/crash harness, native FS | No precommit mutation, verified commits only, recoverable backup, uncertain state no blind retry; native metadata policy |
| VO-06 | REQ-PATH-01/02, REQ-MAP-01 | Directory fixtures, real OS, fuzz path inputs | First valid root, manual override, scopes/nesting/alias safety and distinct context |
| VO-07 | REQ-MPQ-01/02 | StormLib differential, fuzz, bounded offset Kani if owned, native tests | Correct read and authorized copy output, bounds, no source modification; upstream library limitations recorded |
| VO-08 | REQ-MAP-02/03, REQ-PERF-01 | Criterion, trace/measure native, Shuttle/Loom | No duplicated parsing, immutable lookup, worker cap/backpressure and cancellation; truthful memory/runtime results |
| VO-09 | REQ-UI-01/02/03/04/05/06/07/08/09 | Vitest, Playwright, native e2e, visual snapshots | Exact approved layout/copy/menus, real data, accessibility, correct numeric/Raw edits and responsive scrolling |
| VO-10 | REQ-WEB-01/02, REQ-CLI-01 | WASM/native/CLI contract tests, browser matrix, security audit | Same engine outputs, no network data, scoped commands, safe permission fallbacks, dry-run no side effects |
| VO-11 | REQ-DIST-01, REQ-MIG-02, REQ-SCOPE-01 | Native install tests, dependency/provenance/security audit | File association cold/warm, no destructive uninstall, lawful distribution, no network/process scope creep |
| VO-12 | REQ-MIG-01, REQ-ENG-01/02, REQ-QA-01 | Hash validation, fmt/lint/structural checks, CI and test audit | Archived Go unchanged; evidence honest, required tests mapped, no fabricated passes |

Formal verification palette is deliberate but selective. Kani proof bounds, Loom/Shuttle schedules, TLC finite state bounds and Lean/hax/Aeneas extraction limits must be explicit. If tool incompatibility blocks a specific obligation, document substitute evidence and ask before claiming equivalent proof. Real crash/power-loss, Windows ACL and browser durability cannot be proven by a model alone. See EVS-1.

VO-01 has scoped Miri evidence for 46 parser and writer cases. The scalar and structural AddressSanitizer runs processed 538,990 inputs in two 90-second runs. The namespace declaration budget case passed natively and has no interpreter result. [Miri evidence and remaining tool scopes](../../crates/formats/sc2bank/verification/MIRI.md) · [Fuzz harness](../../crates/formats/sc2bank/fuzz/README.md)

VO-05 includes a bounded TLA+ model of a protected save or restore with a competing write and a process crash. TLC checked 298 distinct states and exercised three expected counterexamples. Native APFS tests covered process crashes and a kernel short-write failure. Physical power-loss durability remains open. [Save and backup model](../../crates/platform/starbank-fs/verification/save-backup-model/README.md) · [Native fault tests](../../crates/platform/starbank-fs/verification/NATIVE_STORAGE.md)

VO-09 and VO-10 include the 10,000-value production browser gate, nested static asset and worker loading under the packaged headers, and offline exact backup and recovery. The frontend CI job checks exact native, release WASM and CLI parity and runs the built desktop renderer with synthetic IPC. The renderer test covers Tools, backup copies and guarded navigation in headless Chromium. [Browser and native verification](IMPLEMENTATION.md) · [Adapter parity](ADAPTER_PARITY.md) · [Desktop renderer test](../../apps/starbank-web/test/desktop-renderer.mjs) · [Chromium timing](BROWSER_PERFORMANCE.md)
