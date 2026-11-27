# Technology decisions and dependency gates

Rust/Tauri migration accepted. Select exact dependency releases after checking official docs/source, Rust 1.100.0 availability and compatibility; no fabricated version/API claims. Pin in manifests/lockfiles after spikes. This is a dependency shortlist, not a fully resolved lockfile.

| Layer | Choice |
|---|---|
| Native | [Tauri 2](https://v2.tauri.app/), native dialog, clipboard, opener/reveal, file-association support verified for chosen release; minimal scoped plugins |
| UI | React, TypeScript, Vite, Tailwind CSS 4, Radix Dropdown Menu, Tabs and Tooltip at the same versions as Euclid, plus Lucide, pnpm and Biome. |
| State/validation | Zustand for UI projections/drafts; Valibot (not Zod) for runtime input/config/DTO shapes; Rust authoritative for document rules. |
| Table | Native table semantics and windowed rows. Add a table or splitter dependency only for a demonstrated gap. |
| Numeric | [react-number-format](https://github.com/s-yadav/react-number-format) string-based integration, no floatValue reliance |
| Raw | CodeMirror 6 + XML language package (verify APIs), lazy load; no Monaco or embedded second webview unless justified |
| Core | `quick-xml` (lossless/event-based spike), `sha1`, `hex`, `serde`, `serde_json`, `thiserror`, `tracing`; YAML profile parser only maintained audited crate (legacy YAML import); avoid unnecessary regex/serde DOM duplication |
| Native discovery | `directories` / platform APIs, `windows` or `windows-sys` narrowly scoped, `notify` if file watcher needed; Linux VDF parser deferred |
| MPQ | `wow-mpq`, `stormlib-rs` main candidates, smaller reader alternatives; StormLib differential reference / optional FFI fallback; see DISCOVERY_MPQ |
| Storage | `atomic-write-file` candidate, `tempfile`, integrity hash (`blake3` or SHA-256), explicit native metadata policy; no Windows atomicity guarantee before real tests |
| Concurrency | bounded standard/native task/worker channels, Rayon for CPU-only indexing; async runtime only where needed, no runtime in portable format/core |
| WASM | wasm-bindgen, wasm-pack or equivalent reproducible build, Web Worker, exact DTO via serde/typed JS bridge; feature gates for native IO/MPQ |
| CLI | clap, serde JSON output; no service daemon |
| Verification | cargo-nextest, rstest, proptest, insta, cargo-fuzz, criterion, Miri, Loom/Shuttle, Kani, TLA+/TLC, Lean/hax/Aeneas as scoped; Vitest, Playwright, actual native UI/filesystem tests |

No automatic updater, analytics/telemetry, authentication, cloud sync, database server, embedded HTTP server or runtime map scripting. Cache file format can be versioned JSON initially; SQLite only if measured scale justifies it, not a requirement. Measure application memory, responsiveness and bundle size. Browser static hosting via Cloudflare Pages candidate; no uploaded user banks.

## Resolved frontend

Both apps use the same React, TypeScript, Vite, Tailwind 4, Radix, Lucide, Valibot and CodeMirror frontend. The private workspace packages are `starbank-ui`, `starbank-editor`, `starbank-engine-client` and `starbank-platform`. Their exact dependencies are recorded in the pnpm lockfile.

`starbank-ui` groups the app shell, editor, inventory and settings by feature. Its Tailwind theme maps utilities to the runtime color and control tokens used by both apps. The Valibot schemas in `starbank-engine-client` check engine replies before the UI uses them. `starbank-editor` owns view selection and pending drafts in a per-document Zustand store.

`starbank-editor` uses Zustand 5.1.0 to hold the active view and pending drafts per open document. Bank values and revisions remain authoritative in the Rust engine. React Number Format 5.4.5 groups ordinary numeric strings for display. A linear formatter retains the existing grouping for padded or long lexemes. Edit normalization and validation preserve exact digit strings. [Editor package](../packages/starbank-editor/package.json) · [Zustand store API](https://zustand.docs.pmnd.rs/reference/apis/create-store) · [React Number Format string API](https://s-yadav.github.io/react-number-format/docs/numeric_format/#numericformatter-numstring-string-props-numericformatprops--string)

The shared bank editor uses native table elements and windowed rows to keep exact text inputs, focus and stable-ID bulk selections. The measured 10,000-value synthetic workflow exercised keyboard navigation and bulk edits across row windows. [Table implementation](../packages/starbank-ui/src/editor/table-editor.tsx) · [Verification](verification/IMPLEMENTATION.md)
