# Go prototype inventory

The live checkout at `59979256b046d092c8e776495cc77070e37aa0d2` was archived in `4119421`. Its 37 tracked files and four untracked project files were moved into `original/`. The manifest records their SHA-256 hashes. The local `.gitignore` had one added exclusion for `docs/temp`; the live bytes were retained. Every other archived file matched the uploaded snapshot, including the uncommitted map-cache work and historical plans.

The supplied pack omitted the upstream dump from its `original/docs/` even though the manifest listed it. The file in the live `docs/temp/` matched the recorded checksum. It was moved into `original/docs/` and remains ignored by Git. Mac app bundles, temporary binaries and logs, Finder metadata and pack extraction files stay outside the archive. The Git database remains at the root.

## Archived capabilities

| Source in `original/` | Observed capability | Work remaining |
|---|---|---|
| `go.mod`, `go.sum`, `justfile`, `.air.toml`, `cmd/starbank/FyneApp.toml`, `assets/logo.png` | Go 1.25.1 declared, Fyne 2.7.3, fyne-tooltip 0.4.0, icza/mpq, YAML; build/dev configs and logo | Replace runtime and build with new stack. Verify asset rights before reuse. REQ-ENG-01, REQ-DIST-01 |
| `pkg/bankeditorlib/bank/model.go` | Bank/BankInfo/Section/Key; type constants fixed/flag/int/point/string/text/unit; add/find; pointer parent | Preserve repeated names, multiple child values, empty keys and unknown XML; separate metadata; no implicit deduplication |
| `bank/parse.go`, `bank/xml.go`, `bank/write.go` (under `pkg/bankeditorlib/`) | Read plain XML, dynamic typed attribute, version, signature, ordered writer | Parser selects single `Value` and first attribute, skips children; AddKey/AddSection coalesce names; unknown data lost. Lossless-or-reject model required; pure serialization not disk write |
| `bank/sign.go`, `bank/sign_test.go`, `util/sort.go` | AuthorID/playerID/name prefix; stable clone + ordinal-like comparison; `text` omission; SHA-1 uppercase; tests for basic digest and nonempty signature | Actual C# ordinal UTF-16 and ASCII fallback vs Go byte ordering/UTF-8; no nil BankInfo guard, incomplete context accepted; multi-value support, signature verification, actual known-good SC2 fixtures |
| `bank/path.go`, `util/validate.go` | Windows Documents/OneDrive, macOS Application Support/Documents/iCloud, Linux stub; enumerate all banks folders; handle regex; manual root/Accounts/Banks | Known Folder API; first valid root, no duplicated candidate scans; distinguish file vs folder validation; access failures, nested paths, legacy and nonstandard folders, robust settings override |
| `util/format.go` | Removes commas and spaces | Type-specific exact validation; no lossy float conversion; integrate with UI and engine; preserve nonnumeric values |
| `internal/editor/editor.go` | Open, metadata, original XML, dirty state, Reset, Save, SaveAs, LoadXML, CopyXML; temp+sync+rename | Reset/save baseline decisions; no safe Windows guarantee, chmod errors ignored; backup overwrites `.bak` and uses cached bytes; SaveAs signs old context before updating it; copy skips re-sign; fail-safe transactional commit, conflict detection, backup/restore, no state changes on failure |
| `internal/maps/maps.go` | Enumerates immediate author folders and bank files, stable display sorting | Direct/legacy/nested banks, all account scopes, unknown/unmapped banks, group actual maps and multiple files; can’t show maps without banks yet |
| `internal/mapinfo/cache.go` | Read-only icza/mpq scan for `.s2ma`, map name/author in `MapScript.galaxy`, literal/variable BankLoad references, Blizzard author detection | Persistent cache, DocumentHeader fallback, ambiguity tracking and bounded index work. REQ-MAP-01 through REQ-MAP-03, REQ-MPQ-01 |
| `internal/schema/schema.go`, `embed.go`, `schemas/README.md` | Embedded YAML, groups, label/section/key/type mapping; number, checkbox, string; case-insensitive bank basename matching | No actual schema files in uploaded snapshot; no profile binding to live bank; version/identity matching, dependent rules, typed transactions; migrate compatible YAML |
| `internal/settings/settings.go` | JSON persistence, three true defaults, root override; OS config directory `StarBank-Reloaded/settings.json` | Versioned settings migration, full page, layout/appearance/backup preferences, no silent reset of overrides |
| `internal/ui/app.go`, `state.go`, `sidebar.go`, `header.go` | Startup discovery, map filters/search, settings folder chooser, real editor open/save callbacks, fixed mock header metadata/bank dropdown; window 850×500 | All metadata and bank selection bound to real data, async startup, safe switching; proper reveal, exact empty-state copy; top-right tools are stubs |
| `internal/ui/views/{simple,table,raw}.go` | Three layout prototypes | All three use static mock content, no synchronized model edits. Raw change only calls edit handler, does not call LoadXML. Table entries do not update model. Simple fields placeholders. Implement live editor rather than translating mock |
| `internal/ui/widgets/{breadcrumb,savebar}.go`, `utils/helpers.go` | Scroll breadcrumb, Save split/menu/copy, tooltip layer | Responsive CSS, hidden scrollbars, all buttons accessible, proper menu positioning/close; avoid historic Fyne workarounds |
| `docs/{PLAN,ARCHITECTURE,TASKS}.md`, local upstream C# dump | Three legacy plans and a pre-v2.0 source reference | Current requirements supersede the plans. Pinned v2.0 sources are tracked in [UPSTREAM.md](UPSTREAM.md). |

The lossless format and signature gaps map to REQ-FMT-01 through REQ-FMT-04 and REQ-SIG-01 through REQ-SIG-03. Discovery maps to REQ-PATH-01 and REQ-PATH-02. Editor behavior maps to REQ-EDIT-01 through REQ-EDIT-07 and REQ-PROF-01 through REQ-PROF-02. Persistence maps to REQ-SAVE-01 through REQ-SAVE-02 and REQ-BACKUP-01 through REQ-BACKUP-02. The archive has no CI workflow or end-to-end GUI tests. The existing Go tests exercise parsing, basic round-trip and signing, with no SC2-generated compatibility evidence.

## Provenance

`original/LICENSE` retains the prototype's MIT copyright notice. The historical upstream source dump, possible source translations, asset rights and third-party obligations still need review for REQ-MIG-02. The release license requires owner approval. The upstream dump remains local pending that review.

## Verification status

- Before the move, `GOWORK=off GOTOOLCHAIN=local GOPROXY=off go test -count=1 ./...` passed on Go 1.27.0 for macOS arm64.
- From `original/`, `GOWORK=off GOTOOLCHAIN=local GOPROXY=off go test -mod=readonly -count=1 ./...` passed. `go build -mod=readonly -o <temp-path> ./cmd/starbank` compiled the GUI with the same environment. The linker reported a duplicate `-lobjc` warning.
- The staged archive matched all 41 pre-move hashes. A disposable copy of the supplied pack, with the recovered ignored dump, passed its original validator for 42 snapshot entries. The imported validator checks the live archive and verifies the ignored dump when present.
- The declared minimum Go 1.25.1, GUI runtime behavior and Windows builds remain unverified. For archived development, start in `original/` before running `air` or a platform recipe in its `justfile`.
