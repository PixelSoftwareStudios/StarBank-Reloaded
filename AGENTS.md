# AGENTS.md — StarBank Reloaded

## Mission and authority

Implement a dependable, lightweight SC2 bank editor with StarBank v2.0 parity plus Reloaded features, Rust + Tauri + React + WASM. Read the [roadmap](docs/roadmap.md), [architecture](docs/architecture.md), [work packages](docs/work-packages.md), and the relevant requirements and standards before editing. Current owner decisions override historic Go designs. Keep the Go prototype in `original/` as an archive and develop the new editor from format requirements and authorized fixtures. Do not claim unimplemented features or tests passed.

## Mandatory first task

Follow [the migration record](docs/reference/MIGRATION.md): inspect the actual existing repository; preserve Go/Fyne code, assets, module manifests, tests, license, old docs and dev configs under `original/` with Git history retained. Preserve hidden files that belong to the project, not secrets, personal data or build output. Never move `.git`, destroy untracked files, rewrite history, or commit a source snapshot that was guessed from chat. Report gaps between the prototype and actual code. Do not scaffold the new root until the move is verified. The supplied source snapshot is already archived in this pack. Reconcile it with the live checkout before implementation; never replace newer local work blindly.

The live move was verified in `4119421`. See `docs/reference/GO_INVENTORY.md`; keep the archived bytes intact.

## Non-negotiable engineering rules

- Rust 1.100.0 pinned, edition 2024, Cargo resolver 3, one workspace and root lockfile. Group dependencies by purpose; use workspace inheritance for shared deps/package metadata/lints. Pin tool versions and lockfiles; no blind upgrades.
- Use hard tabs in authored Rust source, `rustfmt.toml` (`hard_tabs = true`, `tab_spaces = 4`). `cargo fmt --all -- --check` is required. Follow language conventions in non-Rust formats; use Biome for TS/JS/JSON.
- No magic strings or unexplained magic numbers. Use typed enums/newtypes for closed domains, named constants for format tags, protocol keys, config names, meaningful limits and shared literals. Preserve unknown on-disk type tags explicitly. Ordinary single-use UI copy need not become a constant.
- `lib.rs` and every `mod.rs` are structural only: module declarations, re-exports, docs, crate attrs and structural feature/visibility wiring. No types, algorithms, traits, functions, constants, tests or domain behavior there. Keep `main.rs` initialization/delegation only. Typically <=500 authored Rust lines per file, split by responsibility (never arbitrary fragments). Generated/vendor files excluded; document justified exceptions.
- No generic `util` dumping ground; `starbank-core` is low-level shared invariants, not a separate `starbank-types` crate or a catch-all. Keep independently useful formats separate from editor UI; introduce a new crate only for a coherent independently testable boundary. Avoid cyclic dependencies.
- Prefer safe Rust, explicit ownership, checked arithmetic, typed errors with context, deterministic pure functions. `#![forbid(unsafe_code)]` for pure crates. Any FFI/unsafe exception isolated, documented, audited and tested. No panic/unwrap on untrusted production input. Avoid arbitrary runtime execution, opaque mutation and unnecessary allocation.
- UI must not duplicate Rust format/signature rules. Use a revisioned document engine and explicit DTOs. Preserve unknown XML and exact untouched data; fail closed if safe round-trip cannot be guaranteed. Never write a partially validated bank. File operations must not silently change security metadata or overwrite externally changed data.
- Local-first; no remote bank processing, telemetry, login, embedded HTTP server or network requirement for editing. Scope Tauri capabilities narrowly. Never run game scripts or attach to game processes. Map repair only for authorized local copies; no evasion or runtime tampering.
- No unnecessary dependencies. Use React, TypeScript, Vite, Tailwind 4, Radix primitives, Valibot, Zustand, Lucide and CodeMirror 6. Use native table semantics and add a table dependency only for a demonstrated gap. React Number Format or compatible input wrapper must preserve exact numeric lexemes. Avoid Next.js/SSR. Match the approved compact layout, not a generic dashboard.
- **Change scope:** never make unrelated layout, labels, menu, data or behavior changes. Keep approved UI and behavior while fixing one issue. Do not replace native-style in-window dropdowns with unrelated custom controls. No `// NOTE:` insertion. Use actual current library APIs and check deprecations/warnings. Explain any necessary incidental change explicitly.
- Use reproducible behavior and independently documented format evidence for compatibility claims. Verify dependency existence, licenses, versions, supported targets, APIs and limitations in primary documentation/source. Unknown is unknown; do not invent library symbols or benchmarks. Do not label this work clean-room without evidence.

## Implementation and evidence workflow

1. Select one work package; cite requirement IDs, write plan/acceptance cases and inspect the current code before changes.
2. Keep changes small and atomic. Use Conventional Commits (`feat(bank): ...`, `fix(signature): ...`, `test(writer): ...`, `docs: ...`). A single purpose per commit, related tests included. No unauthorized pushes, force-pushes, branch deletion, releases or history rewrites.
3. Parallel agents use disjoint owned modules/branches/worktrees, agreed contracts, and one integrator for shared manifests and schemas. Never overwrite concurrent edits. Resolve conflicts explicitly; rerun integration gates.
4. Tests and proofs live with the crate owning the invariant (`tests/`, `benches/`, `fuzz/`, `verification/`). Test code at least comparable to authored production logic; target 2:1 for safety-critical format/signing/persistence logic as a review signal, not a reason to generate meaningless tests. Record genuine evidence, commands, toolchains, fixtures and environment. Never mark simulated results as executed.
5. Gates: fmt, Clippy with `-D warnings` on relevant supported feature/target matrix, cargo check/test/nextest, docs, TypeScript typecheck, Biome, frontend tests, WASM and native build, dependency/license review, security scans. Run applicable property/fuzz/concurrency/formal jobs; report explicit exclusions. CI should block regression on safety obligations. See EVS-1.
6. Summarize changes, exact verification results, failures, decisions and remaining blockers. Leave a usable next step; no fabricated code or claims of compatibility.
