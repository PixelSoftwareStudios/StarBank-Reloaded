# Migration of the Go prototype

The live Go project was archived under `original/` in commit `4119421`. The included architecture pack was compared against the actual checkout before the move. See the [Go inventory](docs/reference/GO_INVENTORY.md) for the source differences, verification and open provenance work.

## WP-01 (first commit)

1. Inspect `git status --short`, `git ls-files`, project tree, `go.mod`, `go.sum`, `cmd/starbank/main.go`, `pkg/bankeditorlib/`, Go tests, `.air.toml`, Fyne packaging metadata/assets, license, CI and docs. Identify any implemented parsing, signing, sorting, resolver, writer/MPQ code and their exact status. Do not assume chat snippets were committed. Check ignored/untracked files for useful project artifacts; exclude secrets, binaries, logs, personal bank files and caches.
2. Record baseline commit, file list and hashes; preserve work on a migration branch. Keep `.git` at root. Move actual Go-owned tracked paths with `git mv` into `original/`, retaining relative structure; move untracked project files only after owner approval. Inventory shared/root files and move/keep individually (license notices and CI should not disappear). Do not introduce a nested `.git`. Preserve the old build/development instructions and module name; record commands relative to `original/`.
3. Compare the included `original/` snapshot with the local working tree; preserve any newer local work. Compare contents before/after; verify Go build/tests if toolchain and native dependencies permit. Record failures as pre-existing or migration-specific, never silently fix behavior. Do not change Go code just to migrate it. New Rust CI must not depend on the archived Go application. If using old functions as differential oracles, explicitly isolate and document provenance.
4. Commit the move separately, e.g. `chore: archive Go prototype under original`. Then import this pack's current docs and configs without overwriting project-specific root files blindly. Keep legacy docs separate and marked superseded. Establish the new workspace after WP-02, in another atomic commit.
5. Produce `docs/reference/GO_INVENTORY.md` with actual paths, status (implemented/mock/missing), test evidence, portability issues, and mapping to REQ IDs. Preserve any feature found in the real Go repository that isn't captured here; add a requirement rather than dropping it. Do not publish private paths, account numbers or bank data.

## Expected historical details to reconcile

- Go 1.25.1, Fyne v2.7.3 (earlier prototype v2.7.1), fyne-tooltip v0.4.0, Air built `./cmd/starbank`, mock initial window 850×500.
- `pkg/bankeditorlib` modeled Bank/Section/Key/BankInfo, pointer relationships, XML, SHA-1, stable non-mutating sorting and path resolver plans. Latest upstream requires multi-value keys, so old model cannot be copied as-is.
- `GetSC2RootPath`, `GetBankFolderFromRoot`, `IsValidBankPath`; Linux stub, manual fallback. Old docs conflict about macOS path and first bank folder vs all account folders; current resolver design wins.
- Raw editor and save actions mostly mocked in the visible GUI. Copy XML worked on a string; resetting dirty flag is not true reset. Preserve design, not mock limitations.
- Old GUI had real `fyne.Menu` popups; do not regress to custom stuck popups. No account label or branding footer in default layout; see UX spec.

No migration scripts here execute a move automatically: only the repository-aware agent should perform it with complete visibility. Do not treat the archived prototype's correctness as established.
