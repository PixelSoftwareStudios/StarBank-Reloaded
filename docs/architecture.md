# ARCHITECTURE.md — Boundaries, workspace, data flow

## Architecture

One Rust domain engine, a shared UI, three entry points: native desktop (Tauri IPC), browser (WASM in a worker where appropriate), and CLI. Filesystem access is a platform adapter, never a prerequisite for the pure parser/signer/editor. Browser imports bytes or authorized file handles and exports bytes; no remote processing. CLI is a thin adapter over the same services. The native side owns desktop document state; browser worker owns its document state. React maintains a revisioned projection and draft input, not a second canonical model.

```text
React shared UI (desktop/web)                CLI
       | typed client / revision DTOs          |
 Tauri commands       WASM bindings            |
       |                   |                   |
       +--------- application/editor ---------+
                    |             |
            profiles/validation   signing
                    |             |
                    +--- sc2bank -+
                          |
                    starbank-core

Native services: paths -> bank inventory -> map index -> MPQ adapter
                 editor -> filesystem commit -> backup storage
Browser services: file picker/download/clipboard -> platform adapter
```

## Intended monorepo tree (stage by responsibility, not empty crates)

```text
starbank-reloaded/
├── AGENTS.md
├── Cargo.toml                       # resolver 3, shared metadata/deps/lints
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── package.json                     # pnpm workspace scripts
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
├── biome.json
├── original/                       # verified Go archive; excluded from new builds
├── apps/
│   ├── starbank-desktop/            # Vite shell
│   │   ├── src/
│   │   └── src-tauri/               # Cargo workspace member, thin commands
│   ├── starbank-web/                # Vite static shell
│   └── starbank-cli/                # Cargo workspace member
├── packages/
│   └── starbank-ui/                # shared frontend, organized by feature
│       └── src/
│           ├── editor/             # components, Zustand state and presentation
│           ├── engine/             # typed worker client and Valibot protocol
│           ├── platform/           # local browser IO and bank encoding
│           └── settings/           # settings components and Valibot model
├── crates/
│   ├── foundation/starbank-core/   # minimal common IDs, limits, primitives, errors
│   ├── formats/sc2bank/             # independent format reader/writer and model
│   ├── domain/starbank-signature/   # SC2 compatibility signing; not XML hashing
│   ├── domain/starbank-editor/      # revision, commands, reset/diff/history
│   ├── domain/starbank-profiles/    # declarative game-specific interpretation
│   ├── domain/starbank-backup/      # verified backup/restore catalog policy
│   ├── platform/starbank-fs/        # actual filesystem commit/metadata/watch boundary
│   ├── discovery/starbank-paths/    # root/bank paths, OS integration
│   ├── discovery/starbank-map-index/ # bounded scanning and metadata association
│   ├── adapters/starbank-mpq/       # narrowly scoped archive capability boundary
│   ├── bindings/starbank-wasm/      # wasm-bindgen only, pure engine facade
│   └── engineering/starbank-testkit/ # shared fixtures/generators, dev-only
├── docs/                            # requirements/design/verification/ADRs
└── .github/workflows/               # native and WASM CI/release
```

`sc2bank` should be independently usable, with a small published API; if separate publication is desired, its dependency boundary must stay clean (do not require app-specific runtime). A separately maintained MPQ implementation is outside this product; use existing libraries. Do not create a crate merely to wrap a trivial function. No standalone `*-types` or codec catch-all. `starbank-core` is lowest-level: typed IDs, bounded sizes, common invariant/errors; no UI, async runtime, MPQ, platform or higher-layer dependencies. A distinct `starbank-fs` is justified by external effects and recovery obligations; backup policy depends on a narrow storage trait/adapter, not vice versa. Use dev-only testkit, avoid production cycles.

## Suggested file organization

Each crate has `src/lib.rs` structural only, responsibility-specific modules (`model.rs`, `parse.rs`, `serialize.rs`, `validate.rs`, `error.rs`, `limits.rs`, `context.rs`, `canonical.rs`, `digest.rs`, `command.rs`, `session.rs`, `history.rs`, `diff.rs`, `transaction.rs`, etc. as relevant), local tests, benchmarks and verification. OS-specific code behind cfg in `platform/{windows,darwin,linux}.rs` or equivalent structural modules. Native-only dependencies never leak into WASM. Define typed closed enums and preserve future unknown values; avoid strings for closed error/status/command types. No parent-pointer cycles; ordered vectors plus stable opaque identifiers for views. Use OS paths (`PathBuf`, `OsString`) not slash concatenation or lossy round-trip; DTO display path separate from authorized handle.

## Ownership and operations

| Boundary | Owns | Does not own |
|---|---|---|
| `sc2bank` | lossless source/semantic view, typed values, parse/serialize limits | paths, save policies, UI |
| `starbank-signature` | signing context, ordinal order, encoding, digest, diagnostics | XML UI, writes, map-specific guesses |
| editor | single mutation owner, revisions, commands, validated prepare/export, snapshots/history | native path discovery, DOM widgets |
| profiles | match rules, friendly fields, declarative dependent adjustments | runtime game process, arbitrary script execution |
| discovery | actual file inventory and map-association confidence | sign/save decisions, arbitrary archive writes |
| MPQ adapter | open/read/existence/authorized copy/export capability | bank XML, cache policy |
| filesystem + backup | effects, permission policy, conflict checks, durable outcomes, restore | business rules/signature computation |
| UI | projections, input drafts, selection, accessible layouts | authoritative domain invariants |

## Contracts (illustrative, names may be idiomatic Rust)

- `parse(bytes, limits) -> Result<BankDocument, ParseError>`; `serialize(document, policy) -> Result<Vec<u8>, SerializeError>`; `validate(document) -> ValidationReport`.
- `verify_signature(document, context) -> SignatureStatus`; `compute_signature(...) -> Result<Signature, SignatureError>`; `prepare_export(revision, options) -> Result<PreparedBank, PrepareError>`.
- `open(source, context_hint) -> DocumentHandle + Snapshot`; `apply(handle, expected_revision, EditTransaction) -> EditOutcome`; `undo/redo/reset`, `diff`, `close` and `snapshot`.
- `discover_root() -> Result<PathBuf, DiscoveryError>` first valid; `banks_from_root(root) -> Result<Vec<BankRoot>, DiscoveryError>`; `validate_bank_path(path) -> PathValidation`. Historical Go names `GetSC2RootPath`, `GetBankFolderFromRoot`, `IsValidBankPath` mapped in discovery spec. Distinguish manual-folder validity and signable file-path validity.
- `commit(prepared, target, expected_fingerprint, backup_policy) -> CommitOutcome` (explicit confirmed/failed/uncertain states), `restore(backup_id, expected_target)`.

## Data and event flow

Startup loads settings and renders shell immediately; discovery and indexing run on bounded background workers, publish batches with generation tokens, UI discards stale results. Selecting a map/account/bank opens only that document; one immutable opened baseline plus current model and last-saved fingerprint. Every edit is a typed transaction tied to a revision, with affected projections and diagnostics. Save, Save As, backup, copy, re-sign, CLI export all share preparation rules. File I/O is separate and never starts before validation/signing succeeds. Browser file handles and clipboard are platform side effects, not engine responsibilities.

Performance: no eager XML for every bank; load metadata only. Use cached sorted indices / stable borrowed sorting and display-only formatting. Keep original bytes or immutable shared buffer needed for reset and fidelity; lazy raw text editor, release redundant drafts when safe. Bounded history and cache; streaming parsers/hash where applicable; no unsafe micro-optimizations without measurements. No global Rayon pool oversubscription or synchronous heavyweight IPC work on UI thread. WASM single-thread worker baseline, optional parallelism only with tested browser isolation.

## Decisions and dependencies

Refer to `docs/TECH_STACK.md` and ADR register. Radix primitives keep the keyboard and focus behavior shared between the Vite and Tauri hosts. Exact versions are pinned in the workspace lockfile. Native MPQ backend selected by actual compatibility results, not documented feature count; browser archive support optional. Use Tauri's system webview, not a bundled Chromium or sidecar HTTP service. Native and browser adapters communicate through the same versioned DTO schema. Provide capability flags instead of assuming all platform APIs exist.
