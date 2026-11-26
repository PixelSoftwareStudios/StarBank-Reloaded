# Acceptance register

This register maps one minimum acceptance case per requirement. Completed entries link to the evidence.

| Test | Requirement | Acceptance | State |
|---|---|---|---|
| AT-MIG-01 | REQ-MIG-01 | Reconcile the uploaded and live files. Verify the archived bytes and preserve newer edits. | Passed locally. [Go inventory](../reference/GO_INVENTORY.md) |
| AT-MIG-02 | REQ-MIG-02 | Inventory and license/notice audit recorded; owner approves release license before publication. | Pending |
| AT-ENG-01 | REQ-ENG-01 | CI gates formatting, lints, module-root and size checks; no unsupported APIs or invented results. | Local gates passed. Hosted matrix pending. [Evidence](BOOTSTRAP.md) |
| AT-ENG-02 | REQ-ENG-02 | Dependency spikes verify real source, license and targets; no Zod/shadcn default or empty crates. | Initial dependencies and shell verified. Production target qualification pending. [Evidence](BOOTSTRAP.md) |
| AT-FMT-01 | REQ-FMT-01 | Read ordinary real/synthetic XML and reparse exported bank; no MPQ dependency. | Synthetic UTF-8 tests passed. Real SC2 corpus pending. [Evidence](IMPLEMENTATION.md) |
| AT-FMT-02 | REQ-FMT-02 | Round-trip multi-value, empty and repeated-name fixtures without coalescing; child labels visible. | Format tests passed. Editor projection pending. [Evidence](IMPLEMENTATION.md) |
| AT-FMT-03 | REQ-FMT-03 | Unchanged export byte identical; edited round-trip preserves semantics/unknowns or rejects. | Unchanged and targeted attribute edits passed. Other encodings and structural edits pending. [Evidence](IMPLEMENTATION.md) |
| AT-FMT-04 | REQ-FMT-04 | Malformed/truncated/oversize/DTD inputs safely reject; fuzz corpus and limits tested. | Bounded rejection tests passed. Sustained fuzzing pending. [Evidence](IMPLEMENTATION.md) |
| AT-SIG-01 | REQ-SIG-01 | Known-good SC2 fixtures and pinned upstream differential vectors, including multi-value/Unicode, pass. | Four synthetic vectors matched pinned v2.0. SC2 fixtures pending. [Evidence](IMPLEMENTATION.md) |
| AT-SIG-02 | REQ-SIG-02 | External, nested and ambiguous paths require context; no false valid state. | Library validation and status tests passed. Context UI pending. [Evidence](IMPLEMENTATION.md) |
| AT-SIG-03 | REQ-SIG-03 | Prepared export verifies after reparse; context changes and opt-out diagnosed. | Pending |
| AT-EDIT-01 | REQ-EDIT-01 | Switch views without losing edits or writing wrong recycled row; stale commands rejected. | Pending |
| AT-EDIT-02 | REQ-EDIT-02 | 1,000,000 -> 1000000; string commas untouched; >2^53 exact; malformed/group/range invalid blocks save. | Pending |
| AT-EDIT-03 | REQ-EDIT-03 | Compatible rows update together; any invalid target rejects all; one undo restores. | Pending |
| AT-EDIT-04 | REQ-EDIT-04 | Reset no IO; baseline remains recoverable, saved-state comparison accurate; added/deleted/changed visible. | Pending |
| AT-EDIT-05 | REQ-EDIT-05 | No accidental loss or stale row mutation; stable display order; exact raw order preserved. | Pending |
| AT-EDIT-06 | REQ-EDIT-06 | Malformed XML remains draft; prompt on switch; valid raw changes visible across tabs. | Pending |
| AT-EDIT-07 | REQ-EDIT-07 | Save/Discard/Cancel works, re-open/compare/save copy; no silent overwrite or dirty clear. | Pending |
| AT-PROF-01 | REQ-PROF-01 | Known tested profile updates all affected values atomically; unknown/mismatch falls back to Table. | Pending |
| AT-PROF-02 | REQ-PROF-02 | Migrate compatible definitions; invalid/cyclic/ambiguous rule rejected, no arbitrary execution. | Pending |
| AT-SAVE-01 | REQ-SAVE-01 | All modes serialize current validated revision; copy neither writes nor clears dirty; destination context before signing. | Pending |
| AT-SAVE-02 | REQ-SAVE-02 | Failure/crash/lock/conflict/ACL tests on actual Windows/macOS; no blind retry or data loss. | Pending |
| AT-BACKUP-01 | REQ-BACKUP-01 | Unique bankname_timestamp.sc2bank.bak; existing bytes preserved, backup failure aborts. | Pending |
| AT-BACKUP-02 | REQ-BACKUP-02 | Batch report, restore verifies and backs up current target, prior backup never overwritten. | Pending |
| AT-PATH-01 | REQ-PATH-01 | Priority tests incl redirected docs, permission failure, empty folder, explicit NotImplemented. | Synthetic candidates passed on macOS. Windows runtime pending. [Evidence](IMPLEMENTATION.md) |
| AT-PATH-02 | REQ-PATH-02 | Manual root/Accounts/Banks/arbitrary collection; no guessed signature context; exact fallback text. | Local selection and context tests passed. UI fallback integration pending. [Evidence](IMPLEMENTATION.md) |
| AT-MAP-01 | REQ-MAP-01 | No discarded collisions; account picker filters only selected scope; unknown metadata visible. | Inventory keeps collisions and hidden banks. Account UI and metadata pending. [Evidence](IMPLEMENTATION.md) |
| AT-MPQ-01 | REQ-MPQ-01 | Differential read, compression/Unicode/error bounds vs StormLib; no MPQ for bank parser. | Pending |
| AT-MPQ-02 | REQ-MPQ-02 | Backend read/write spike and verified copy output; source untouched; no unsupported success claim. | Pending |
| AT-MAP-02 | REQ-MAP-02 | Map grouping, known Blizzard filter, ambiguity resolution and multiple bank selector. | Pending |
| AT-MAP-03 | REQ-MAP-03 | Unchanged map not re-opened, changed re-index, corrupt cache rebuild; 4-worker cap and no freeze. | Pending |
| AT-UI-01 | REQ-UI-01 | Visual snapshots and keyboard tests at 850×500 and narrow/wide; options pinned, list grows. | Browser tests passed at 850×500 and 390×640. Native and wider views pending. [Evidence](IMPLEMENTATION.md) |
| AT-UI-02 | REQ-UI-02 | Filter and setting behavior, layout and copy snapshots; More and gear same settings page. | Synthetic filters and callbacks passed. Settings integration pending. [Evidence](IMPLEMENTATION.md) |
| AT-UI-03 | REQ-UI-03 | Long path/resize/manual scroll, filename visible; icons do not shrink, other scrollbars unaffected. | Pending |
| AT-UI-04 | REQ-UI-04 | Match reference and show unknown/no bank/multi-bank states correctly. | Pending |
| AT-UI-05 | REQ-UI-05 | Actual bank projections, selectable profile, virtualized editing and settings toggle, no mock data. | Pending |
| AT-UI-06 | REQ-UI-06 | XML highlighting/typing/paste, raw validation; numeric grouping/caret and clipboard tests. | Pending |
| AT-UI-07 | REQ-UI-07 | Exact menu items, aligned insets, actionable tooltips, menu keyboard/outside close; status real. | Pending |
| AT-UI-08 | REQ-UI-08 | Native reveal and menus work; collision-aware popup, keyboard and touch accessibility. | Pending |
| AT-UI-09 | REQ-UI-09 | Manual override, system/light/dark, sections toggle, settings restart/migration, no silent reset. | Pending |
| AT-WEB-01 | REQ-WEB-01 | Network inspection confirms local processing; native/WASM parity and offline editing after load. | Pending |
| AT-WEB-02 | REQ-WEB-02 | Chromium/Firefox/WebKit fallback tests; permission denial safe, no native-only feature fake. | Pending |
| AT-CLI-01 | REQ-CLI-01 | Native vs CLI/WASM contracts; dry-run no write; explicit overwrite; documented command/skill. | Pending |
| AT-DIST-01 | REQ-DIST-01 | Windows/macOS install/open/uninstall tests, no forced defaults; Linux manual package where supported. | Pending |
| AT-PERF-01 | REQ-PERF-01 | Reproducible benchmarks and profiles; no invented memory claims; approved budgets. | Pending |
| AT-QA-01 | REQ-QA-01 | All required test/proof evidence or explicit scoped exclusion/blocker; release cannot claim unverified parity. | Pending |
| AT-SCOPE-01 | REQ-SCOPE-01 | Capability/security audit; no remote bank processing or live game interaction. | Pending |
| AT-LATER-01 | REQ-LATER-01 | Fixture-driven first-valid root and external libraries; manual continues working. | Pending |
| AT-LATER-02 | REQ-LATER-02 | Capability-specific tests and safe fallback; no native release blocked. | Pending |
| AT-LATER-03 | REQ-LATER-03 | Prioritize only with owner approval; maintain single-document baseline and existing scope. | Pending |
