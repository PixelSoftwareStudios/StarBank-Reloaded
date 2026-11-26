# PLAN.md — Product and scope

## Product

StarBank Reloaded is a modern replacement for BlueRaja's StarBank editor for StarCraft II custom-map save data. StarBank Web is the browser edition of the same product. Keep the original workflow recognizable while making the interface attractive, responsive, precise and safer. Existing Go/Fyne code goes to `original/`; the new product uses Rust, Tauri 2 and a shared React frontend, with a headless CLI and a portable WASM engine. Users can work without an installed game or network connection. No server handles user banks.

## Tenets

1. Correct compatibility first, then safety, responsiveness and memory efficiency. Do not sacrifice data integrity for performance or appearance.
2. Preserve every accepted feature, including original StarBank 2.0 behavior; use fixtures and source evidence instead of speculation.
3. One authoritative document engine, multiple views; no duplicated editing or signing rules in JavaScript.
4. Local data stays local; no login, analytics, telemetry, cloud sync or server-side uploads.
5. Pretty but compact UI, faithful to the accepted reference. Settings allow substantial customization without destabilizing defaults.
6. Small, focused crates and commits; measured performance; test and verification evidence by default.

## Release slices

| Slice | Required deliverables |
|---|---|
| A — preservation & feasibility | Archive Go; inventory capabilities and licenses; pin upstream v2.0; real fixture corpus; MPQ read/write/WASM and filesystem safety spikes; approve crate contracts |
| B — core | Lossless XML model, parse/write, validation, exact signing/verification, context inference/manual entry, number normalization, transactional editing, safe file operations, backup/restore, CLI |
| C — discovery | Windows/macOS resolver, Linux explicit auto-detect stub with manual support; all accounts, author folders, nested/hidden banks, map cache scanning, persistent metadata, ambiguity resolution, incremental bounded indexing |
| D — desktop & web UI | Three synchronized views, full map navigation/filters, profile system, raw editor, save split button, re-sign action, backup status, settings, drag/drop, copy, accessibility, responsive layout and all tooltips |
| E — original parity | External bank signing, local authorized map repair/export, cached and unknown map handling, account selection, per-map/all backups and reversible restore, source-based compatibility audit |
| F — distribution & polish | Signed/package builds and file associations, static web deploy, real OS/browser tests, performance baselines, docs, agent skill/CLI contracts, settings persistence, no user data in test artifacts |

No release labelled feature parity until all upstream features in `docs/reference/UPSTREAM.md` are verified or explicitly listed as unsupported. Web can release separately as a fully functional local-bank editor with clearly stated filesystem/MPQ restrictions; do not hold parsing/editing hostage to a native archive dependency. Linux auto-detection (Proton/Wine/Lutris) is deferred deliberately, not silently omitted.

## Mandatory product features

- Discover banks even without map metadata; optional cached maps without banks; multiple accounts/characters/regions; manual root selection regardless of auto-detection; player display names when reliable; author and last-download metadata when known.
- Simple View for recognized game profiles (friendly names, derived values, dependent updates); Table View for all typed values, separate Sections sidebar or unified table; Raw View for editable syntax-highlighted XML; one shared document, validation and revision history.
- Type the same value in many selected fields in one operation. Support grouped numbers (`1,000,000`), exact decimal/integer text, strict normalization before write/sign, inline validation; preserve untouched raw values. Sorting for display with optional header chevron next to Name, not a separate stray button. Keep default visual style clean.
- Reset restores original opened values (no disk write), per-field/section revert, undo/redo, dirty state and diff; confirm destructive navigation; handle external changes.
- Save default overwrite; Save As; Save w/ Backup (`bankname_timestamp.sc2bank.bak`); Copy bank XML to clipboard instead of saving; automatic re-sign default on; explicit re-sign current bank. Shared prepare pipeline; validate before commit. Backup before first overwrite per session as in v2.0; settings govern automatic backup policy, not silent removal of safety. Restore backs up current content first. See persistence spec.
- In-window top-right Open in Folder, Tools, Settings with tooltips; Tools: Unprotect external map file, Re-sign external bank, Back up all bank files. Per-map backup/selected-bank restore and signature status. No native system menu required.
- Full settings page reached by gear/More from Quick Options; persistent layout, theme, filters, paths, saving, backup, editor and display options. No visible account footer or app/version footer by default (both explicitly removed); account selection is separate, clean UI.
- CLI and machine-readable JSON/dry-run for users and coding agents; no arbitrary shell or network server. Native file associations, drag-and-drop; offline browser support.

## Scope boundaries and stretch

Map-specific custom data codecs, check codes and profile rules: support declared, understood local formats through tested typed profiles; do not claim universal protection compatibility. Unsupported/unknown/runtime-bound schemes produce clear diagnostics; inspect only authorized files. Do not add live memory scanning, injection, trainer features, anti-cheat bypass or ban evasion. Do not assert that editing is ban-safe. Map repair is scoped to authorized offline copies and a distinct action, not automatic modification of installed/cached maps.

Deferred: Linux auto-detection, optional WASM MPQ support if proven, user-authored declarative profiles/presets and multi-document editing if needed; optional JSON section import/export, favorites/pins, MRU, read-only mode, backup comparisons, bulk arithmetic (checked), advanced profile tooling and PWA install. Prior suggestions not explicitly accepted are tracked separately in `docs/reference/CONVERSATION_DECISIONS.md` rather than dropped or silently promoted to launch blockers. No arbitrary scripting/plugin runtime, cloud sync or web server. No automatic updater unless explicitly approved later.

## Licensing

The owner prefers MIT for new work. The upstream tree includes GPL-covered ObjectListView files, and the pinned StarBank app tree lacks a verified license grant. File-level provenance for possible Go translations, asset rights and the dependency audit are open. The release license requires owner approval. See [the upstream audit](docs/reference/UPSTREAM.md) and [ADR 011](docs/decisions/ADR_REGISTER.md).
