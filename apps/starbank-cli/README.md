# StarBank CLI

Build from the repository root with `cargo build --locked -p starbank-cli`. The binary is named `starbank`. It operates on selected local files and can run without StarCraft II or the desktop app.

## Commands

| Command | Result |
|---|---|
| `inspect FILE` | Returns ordered sections, keys and value occurrences, exact XML-decoded lexemes, SHA-256, and signature status. |
| `validate FILE [--require-signature]` | Parses bank XML and checks the stored signature when its context is available. |
| `diff BEFORE AFTER` | Reports changed projected occurrences by zero-based position and whether the exact source bytes changed. |
| `discover [--root PATH]` | Enumerates local bank locations and distinguishes live files from backups. Linux requires `--root`. |
| `edit FILE --patch PATCH [--dry-run] [--output PATH \| --in-place]` | Applies a fingerprint-checked batch of value and structural changes through the editor. |
| `sign FILE [--dry-run] [--output PATH \| --in-place]` | Computes, inserts and verifies the signature for the intended destination context. |
| `export FILE --output PATH [--dry-run]` | Prepares and verifies a new bank copy. |
| `backup create FILE` | Verifies a timestamped adjacent snapshot and records its metadata. |
| `backup list FILE [--verify]` | Lists recorded adjacent snapshots. `--verify` checks their length and SHA-256. |
| `backup restore FILE BACKUP --expected-current-sha256 SHA [--dry-run]` | Verifies the selected backup and its target. An actual restore first commits a reversal snapshot. |
| `map inspect FILE` | Reads declared title, author and bank names from one selected .SC2Map archive. |
| `map scan --root PATH` | Indexes selected map folders or archives without writing a cache. Repeat `--root` for multiple locations. |

Append `--json` for machine-readable output. Explicit signing context takes all three options `--author HANDLE --player HANDLE --bank-name NAME`. When omitted, the program infers context only from a canonical account/bank path, using the intended destination for writing. Edit and export re-sign when context is known unless `--no-resign` is supplied. With an unverifiable preserved signature, an actual write requires `--allow-stale-signature`.

Output creation is exclusive. The two in-place commands require `--in-place` and create a verified adjacent backup before changing the original. `--backup` selects an explicitly requested backup reason for the catalog. Dry-run returns the prepared change and signature status without creating any files. It does not reserve a destination filename. Treat retained recovery artifacts and uncertainty as reasons to inspect current files before retrying.

Adjacent snapshots use a UTC timestamp and a collision suffix when needed. The JSON sidecar stores the original basename, canonical signing context when known, timestamp, reason, byte length and digest. Restore requires the matching recorded sidecar and the current live-bank SHA-256. Listed snapshots start unverified in each invocation until checked.

In-place writes and restores check that native storage supports replacing the live bank before creating an adjacent backup. A dry-run can preview the bytes even when the target is read-only.

Native file reads and exclusive output creation are available on macOS and Linux. In-place replacement is currently limited to supported local APFS storage. The Windows adapter supports local read-only snapshots and discovery. Windows writes require a qualified replacement backend.

## Map metadata

Map commands read bounded MPQ metadata from explicitly selected local files or folders. Results describe declarations in archive text and headers. Declared author names and bank references are metadata hints. Galaxy scripts are parsed as data. Both commands use read-only archive handles. Scans run with the cache disabled.

`map_inspect` reports `path`, `modified_unix_nanos` as an exact decimal string when available, `title`, `title_source`, `author`, `bank_names`, `has_script` and `declared_blizzard`. When metadata is unavailable, optional fields are null and the bank list can be empty. `map_scan` reports a list of the same records, typed issues and the `incomplete` flag. Partial scans with some successful archives return their records with exit 0. With only failures, a scan returns the first issue's nonzero status. Check `incomplete` before treating results as exhaustive.

## Versioned JSON

A command report has `schema_version`, `command` and `result` fields. Precondition and operation errors have `schema_version` and an `error` with a stable machine-readable `code`, a message and available diagnostic or recovery fields. Validation can produce a command report with a nonzero exit status. Check the exit status before consuming an intended result.

| Status | Meaning |
|---|---|
| 0 | Success, including a prepared dry-run. |
| 2 | Invalid invocation. |
| 3 | Invalid bank, edit, signature, context, backup or map input. |
| 4 | File or output I/O failure. |
| 5 | Fingerprint conflict or existing destination. |
| 6 | Publication may have occurred and needs inspection. |
| 7 | Unsupported format, storage capability or platform feature. |

The version 1 patch schema uses the exact byte digest reported by `inspect`. Locate a value by zero-based `section`, `key` and `value` positions in the ordered `contents` arrays for that digest. Lexemes are JSON strings.

For example, on a synthetic bank:

```sh
starbank inspect sample.sc2bank --json
starbank edit sample.sc2bank --patch edit.json --dry-run --output edited.sc2bank --json
```

An `edit.json` file looks like this after substituting the digest from the first command:

```json
{
  "schema_version": 1,
  "expected_sha256": "<64 hexadecimal characters>",
  "operations": [
    { "op": "set_value", "section": 0, "key": 0, "value": 0, "lexeme": "1,000,000" }
  ]
}
```

The schema also accepts structural operations. Every position refers to the original fingerprinted input, including after an earlier operation moves an occurrence. To refer to a newly added occurrence, inspect the next input and build a new patch with its fingerprint.

| Operation | Additional fields |
|---|---|
| `add_section` | `name` |
| `remove_section` | `section` |
| `rename_section` | `section`, `name` |
| `add_key` | `section`, `name` |
| `remove_key` | `section`, `key` |
| `rename_key` | `section`, `key`, `name` |
| `add_value` | `section`, `key`, `name`, `kind`, `lexeme` |
| `remove_value` | `section`, `key`, `value` |
| `rename_value` | `section`, `key`, `value`, `name` |
| `set_value_type` | `section`, `key`, `value`, `kind`, `lexeme` |

The maximum patch size is 1 MiB with at most 4,096 operations. Unknown patch fields, duplicate value edits and stale fingerprints are rejected before a write. A fingerprint mismatch returns exit 5 with `input_changed` even if the newly observed bank bytes no longer parse.
