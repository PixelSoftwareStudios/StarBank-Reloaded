# Test plan and release evidence

Read EES-1/EVS-1 and registers. Every Required requirement has a minimum acceptance case in ACCEPTANCE_REGISTER. Test at the owning boundary, then real integration. Crate-local test artifacts: `tests/fixtures`, `tests/`, `benches/`, `fuzz/`, `verification/` as applicable. Shared `starbank-testkit` dev-only fixture utilities. Generated/synthetic data marked; never record personal bank contents, account IDs, unapproved copyrighted map payloads or paths in test logs.

## Test layers

| Suite | Cases |
|---|---|
| Format | XML ordinary/multi/empty/repeated, unknowns, namespaces, escaping, encoding, invalid roots/DTD, excessive limits. Untouched bytes identical; exact semantics on edits. |
| Signature | Known digest inputs, actual SC2 signatures + independent upstream reference, context name/author/player, stable equal order, ordinal supplementary Unicode, ASCII fallback, text skip, no value child, missing attributes. No invented fixture signatures. |
| Editor | Raw valid/invalid switch, dirty reset vs original/last save, per-field/section revert, bulk same value, mixed selection, exact numeric input commas/precision, no type coercion, profile transaction cycles, stale revisions and undo history. |
| Storage | Overwrite/new/export/copy, Save As destination context, original permissions incl ACL limitation, locked Windows file, full disk, temp sync/rename/verify, backup first overwrite/explicit collisions, restore current snapshot, symlink/hardlink, changed target, interruption and recovery. |
| Discovery | Windows Known Folder/OneDrive/macOS Application Support/Documents/iCloud fixtures; all account handles, nested/direct/legacy, unknown roots, permission/empty states, Linux stub; manual selection on all. |
| MPQ | Native and optional WASM on same authorized SC2 map corpus, diff against StormLib, large/Unicode/missing/corrupt/protected archives, unsupported reported; authorized output copy verification. |
| Index | Many cached revisions, 9MB-class script and repeated BankLoad refs, variable resolution, no collisions dropped, cache unchanged/changed/corrupt, progress/cancel, no stale results, concurrency bound and no UI block. |
| UI | Visual baseline 850×500, narrow/wide, system/light/dark, exact labels/insets, search/filter, scroll-to-end and hidden breadcrumb scrollbar only, real metadata, no footer/account label, dropdown close/focus/position, all tooltips, tabs/virtualization and edit flow. |
| Platforms | Actual Windows/macOS UI, file associations cold/warm, native reveal/clipboard/dialog; Linux manual support. Browser Chromium/Firefox/WebKit capability matrix; drop, file picker/download, clipboard, permission denial, offline after initial load, no data transmitted. |
| CLI | Versioned output schema, JSON exact strings, exit codes, dry-run zero writes, explicit overwrite, signed context and diff/backup parity. |

## Verification and performance measurements

Run fmt, Clippy, cargo-nextest, cargo test/doc, feature/target checks, wasm build, Biome/tsc/Vitest/Playwright, security/dep review; selective property/fuzz/coverage/mutation, Miri/Kani/Loom/Shuttle, formal model and crash tests as in EVS-1. CI on actual supported OS (native app build/run) and browser target; feature-gated FFI and WASM evaluated separately. Establish baseline cold/warm startup time, time to usable shell, scan throughput, CPU while idle/indexing, RSS/working set of entire desktop process tree, Rust heap and JS/WASM separately, peak memory under large banks, input latency, scroll, signature/serialization, backup IO, frontend bundle sizes. Repeat comparable hardware/release builds; record medians and tail metrics, fixture sizes, environment and variance. Budgets set after baseline, do not use generic claims about Fyne/Wails/Tauri.

## Release gates

- No unresolved data-loss, signature-compatibility or licensing blocker. Test fixtures and evidence with checksums/permissions/source provenance. User-facing limitations for unsupported file/archive schemes and browser capabilities.
- Required acceptance cases pass or owner accepts explicit exception with documented impact; do not advertise full parity if missing upstream functionality. No fake test/proof evidence. Reproducible package build and safe rollback/update plan; no automatic updater required.
- See [GO_INVENTORY.md](../reference/GO_INVENTORY.md) for the Go baseline test and build evidence, including the Go version used.
