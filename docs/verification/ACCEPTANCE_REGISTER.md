# Acceptance register

This register maps one minimum acceptance case per requirement. Completed entries link to the evidence.

| Test | Requirement | Acceptance | State |
|---|---|---|---|
| AT-MIG-01 | REQ-MIG-01 | Reconcile the uploaded and live files. Verify the archived bytes and preserve newer edits. | Passed locally. [Go inventory](../reference/GO_INVENTORY.md) |
| AT-MIG-02 | REQ-MIG-02 | Inventory and license/notice audit recorded; owner approves release license before publication. | Pending |
| AT-ENG-01 | REQ-ENG-01 | CI gates formatting, lints, module-root and size checks; no unsupported APIs or invented results. | Local gates passed. Hosted matrix pending. [Evidence](BOOTSTRAP.md) |
| AT-ENG-02 | REQ-ENG-02 | Dependency spikes verify real source, license and targets; no Zod/shadcn default or empty crates. | Initial dependencies and shell verified. Production target qualification pending. [Evidence](BOOTSTRAP.md) |
| AT-FMT-01 | REQ-FMT-01 | Read ordinary real/synthetic XML and reparse exported bank; no MPQ dependency. | Synthetic UTF-8 tests passed. Real SC2 corpus pending. [Evidence](IMPLEMENTATION.md) |
| AT-FMT-02 | REQ-FMT-02 | Round-trip multi-value, empty and repeated-name fixtures without coalescing; child labels visible. | Format and live Table projections verified on synthetic banks. [Evidence](IMPLEMENTATION.md) |
| AT-FMT-03 | REQ-FMT-03 | Unchanged export byte identical; edited round-trip preserves semantics/unknowns or rejects. | Unchanged, targeted value, structural and signature export verified on UTF-8. Other encodings pending. [Evidence](IMPLEMENTATION.md) |
| AT-FMT-04 | REQ-FMT-04 | Malformed/truncated/oversize/DTD inputs safely reject; fuzz corpus and limits tested. | Bounded rejection tests passed. Sustained fuzzing pending. [Evidence](IMPLEMENTATION.md) |
| AT-SIG-01 | REQ-SIG-01 | Known-good SC2 fixtures and pinned upstream differential vectors, including multi-value/Unicode, pass. | Four synthetic vectors matched pinned v2.0. SC2 fixtures pending. [Evidence](IMPLEMENTATION.md) |
| AT-SIG-02 | REQ-SIG-02 | External, nested and ambiguous paths require context; no false valid state. | Library and browser explicit context tested. Native context flow pending. [Evidence](IMPLEMENTATION.md) |
| AT-SIG-03 | REQ-SIG-03 | Prepared export verifies after reparse; context changes and opt-out diagnosed. | Prepared re-signing and explicit browser export verify after reparse. Native auto-sign and external tools pending. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-01 | REQ-EDIT-01 | Switch views without losing edits or writing wrong recycled row; stale commands rejected. | Table and Raw share a live revisioned engine. Simple View and virtualization pending. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-02 | REQ-EDIT-02 | 1,000,000 -> 1000000; string commas untouched; >2^53 exact; malformed/group/range invalid blocks save. | Exact integer and decimal edits, invalid grouping, Raw validation and values above 2^53 verified. Game-specific ranges pending. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-03 | REQ-EDIT-03 | Compatible rows update together; any invalid target rejects all; one undo restores. | Atomic same-value editing and one-step undo verified. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-04 | REQ-EDIT-04 | Reset no IO; baseline remains recoverable, saved-state comparison accurate; added/deleted/changed visible. | Reset, checkpoints, history and opening diff/revert verified in the engine. All UI commands pending. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-05 | REQ-EDIT-05 | No accidental loss or stale row mutation; stable display order; exact raw order preserved. | Structural operations and stable IDs verified through the engine and CLI. UI controls pending. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-06 | REQ-EDIT-06 | Malformed XML remains draft; prompt on switch; valid raw changes visible across tabs. | Browser draft retention and exact checked Raw replacement verified. [Evidence](IMPLEMENTATION.md) |
| AT-EDIT-07 | REQ-EDIT-07 | Save/Discard/Cancel works, re-open/compare/save copy; no silent overwrite or dirty clear. | Atomic dirty browser switching and native fingerprint guards tested. Full desktop Save/Discard/Cancel and conflict UI pending. [Evidence](IMPLEMENTATION.md) |
| AT-PROF-01 | REQ-PROF-01 | Known tested profile updates all affected values atomically; unknown/mismatch falls back to Table. | Fixture-backed atomic engine tests passed. Simple View and real profiles pending. [Evidence](IMPLEMENTATION.md) |
| AT-PROF-02 | REQ-PROF-02 | Migrate compatible definitions; invalid/cyclic/ambiguous rule rejected, no arbitrary execution. | Legacy migration and rule rejection tests passed on synthetic definitions. [Evidence](IMPLEMENTATION.md) |
| AT-SAVE-01 | REQ-SAVE-01 | All modes serialize current validated revision; copy neither writes nor clears dirty; destination context before signing. | Shared preparation, browser copy/export and CLI effects verified. All desktop modes pending. [Evidence](IMPLEMENTATION.md) |
| AT-SAVE-02 | REQ-SAVE-02 | Failure/crash/lock/conflict/ACL tests on actual Windows/macOS; no blind retry or data loss. | macOS APFS replacement, ACL and conflict tests passed. Native Windows writes and crash recovery pending. [Evidence](IMPLEMENTATION.md) |
| AT-BACKUP-01 | REQ-BACKUP-01 | Unique bankname_timestamp.sc2bank.bak; existing bytes preserved, backup failure aborts. | Unique snapshots, metadata order and failure abort tested through policy and CLI. [Evidence](IMPLEMENTATION.md) |
| AT-BACKUP-02 | REQ-BACKUP-02 | Batch report, restore verifies and backs up current target, prior backup never overwritten. | Verified reversal and restore tested on macOS APFS through CLI. Batch and desktop UI pending. [Evidence](IMPLEMENTATION.md) |
| AT-PATH-01 | REQ-PATH-01 | Priority tests incl redirected docs, permission failure, empty folder, explicit NotImplemented. | Synthetic candidates passed on macOS. Windows runtime pending. [Evidence](IMPLEMENTATION.md) |
| AT-PATH-02 | REQ-PATH-02 | Manual root/Accounts/Banks/arbitrary collection; no guessed signature context; exact fallback text. | Local selection and context tests passed. UI fallback integration pending. [Evidence](IMPLEMENTATION.md) |
| AT-MAP-01 | REQ-MAP-01 | No discarded collisions; account picker filters only selected scope; unknown metadata visible. | Inventory keeps collisions and hidden banks. Account UI and metadata pending. [Evidence](IMPLEMENTATION.md) |
| AT-MPQ-01 | REQ-MPQ-01 | Differential read, compression/Unicode/error bounds vs StormLib; no MPQ for bank parser. | Synthetic StormLib differential and resource-bound tests passed for supported read layouts. Real corpus pending. [Evidence](IMPLEMENTATION.md) |
| AT-MPQ-02 | REQ-MPQ-02 | Backend read/write spike and verified copy output; source untouched; no unsupported success claim. | Pending |
| AT-MAP-02 | REQ-MAP-02 | Map grouping, known Blizzard filter, ambiguity resolution and multiple bank selector. | Association and ambiguity preservation tested. Native selection UI pending. [Evidence](IMPLEMENTATION.md) |
| AT-MAP-03 | REQ-MAP-03 | Unchanged map not re-opened, changed re-index, corrupt cache rebuild; 4-worker cap and no freeze. | Bounded cache reuse, changes, corruption and cancellation verified on synthetic inputs. Real cache and UI performance pending. [Evidence](IMPLEMENTATION.md) |
| AT-UI-01 | REQ-UI-01 | Visual snapshots and keyboard tests at 850×500 and narrow/wide; options pinned, list grows. | Browser tests passed at 850×500 and 390×640. Native and wider views pending. [Evidence](IMPLEMENTATION.md) |
| AT-UI-02 | REQ-UI-02 | Filter and setting behavior, layout and copy snapshots; More and gear same settings page. | Shared browser Quick Options and settings page, exact labels and persistence tested. Desktop settings pending. [Evidence](IMPLEMENTATION.md) |
| AT-UI-03 | REQ-UI-03 | Long path/resize/manual scroll, filename visible; icons do not shrink, other scrollbars unaffected. | Pending |
| AT-UI-04 | REQ-UI-04 | Match reference and show unknown/no bank/multi-bank states correctly. | Pending |
| AT-UI-05 | REQ-UI-05 | Actual bank projections, selectable profile, virtualized editing and settings toggle, no mock data. | Table and Raw use real bank projections. Profiles and virtualization pending. [Evidence](IMPLEMENTATION.md) |
| AT-UI-06 | REQ-UI-06 | XML highlighting/typing/paste, raw validation; numeric grouping/caret and clipboard tests. | CodeMirror Raw editing, validation, exact grouped input and clipboard fallback tested in browser. [Evidence](IMPLEMENTATION.md) |
| AT-UI-07 | REQ-UI-07 | Exact menu items, aligned insets, actionable tooltips, menu keyboard/outside close; status real. | Pending |
| AT-UI-08 | REQ-UI-08 | Native reveal and menus work; collision-aware popup, keyboard and touch accessibility. | Pending |
| AT-UI-09 | REQ-UI-09 | Manual override, system/light/dark, sections toggle, settings restart/migration, no silent reset. | Browser theme, layout and preferences tested across reloads. Native persistence, legacy root authorization and overrides pending. [Evidence](IMPLEMENTATION.md) |
| AT-WEB-01 | REQ-WEB-01 | Network inspection confirms local processing; native/WASM parity and offline editing after load. | Local offline editing and zero post-startup requests verified in three browser engines. Deployment and full transport conformance pending. [Evidence](IMPLEMENTATION.md) |
| AT-WEB-02 | REQ-WEB-02 | Chromium/Firefox/WebKit fallback tests; permission denial safe, no native-only feature fake. | File import/download and denied clipboard fallback verified in three engines. In-place handle qualification pending. [Evidence](IMPLEMENTATION.md) |
| AT-CLI-01 | REQ-CLI-01 | Native vs CLI/WASM contracts; dry-run no write; explicit overwrite; documented command/skill. | Typed local CLI, dry-run and exact engine projection conformance passed on macOS and Linux. Map inspection and Windows execution pending. [Evidence](IMPLEMENTATION.md) |
| AT-DIST-01 | REQ-DIST-01 | Windows/macOS install/open/uninstall tests, no forced defaults; Linux manual package where supported. | macOS host launch and chooser verified; Linux links. Signed packages and install/association flows pending. [Evidence](IMPLEMENTATION.md) |
| AT-PERF-01 | REQ-PERF-01 | Reproducible benchmarks and profiles; no invented memory claims; approved budgets. | Pending |
| AT-QA-01 | REQ-QA-01 | All required test/proof evidence or explicit scoped exclusion/blocker; release cannot claim unverified parity. | Pending |
| AT-SCOPE-01 | REQ-SCOPE-01 | Capability/security audit; no remote bank processing or live game interaction. | Pending |
| AT-LATER-01 | REQ-LATER-01 | Fixture-driven first-valid root and external libraries; manual continues working. | Pending |
| AT-LATER-02 | REQ-LATER-02 | Capability-specific tests and safe fallback; no native release blocked. | Pending |
| AT-LATER-03 | REQ-LATER-03 | Prioritize only with owner approval; maintain single-document baseline and existing scope. | Pending |
