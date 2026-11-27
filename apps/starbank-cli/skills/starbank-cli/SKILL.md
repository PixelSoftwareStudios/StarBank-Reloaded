---
name: starbank-cli
description: Use the StarBank Reloaded CLI to inspect, compare, patch, sign or back up selected local StarCraft II bank files and read selected map metadata. Use for SC2Bank XML operations with exact values, fingerprint checks and dry-run.
---

# StarBank CLI

Use the installed `starbank` binary. From a StarBank Reloaded source checkout, build it with `cargo build --locked -p starbank-cli`. Check `starbank --help` for the installed command surface.

Use `starbank inspect FILE --json` to obtain exact lexemes, ordered occurrences and the bank's SHA-256. Paths and file contents in results are local user data. Use only the information needed for the user's request.

For edits, create a version 1 JSON patch with the inspected `expected_sha256` and `operations`. A value edit has `op: "set_value"`, zero-based `section`, `key` and `value` positions, and a string `lexeme`. Derive positions from the matching ordered `contents` arrays and preserve precision in strings. Preview with `starbank edit FILE --patch PATCH --output NEW --dry-run --json`. Use a new output path for an authorized write unless the user explicitly asks to replace the live bank.

Signatures require the exact intended author handle, player handle and bank name unless the target path supplies canonical context. When context is uncertain, report the verification state and obtain the missing fields before signing.

For restore, use `starbank backup list FILE --verify --json` and a fresh inspection of the live bank. Preview the selected adjacent snapshot with `starbank backup restore FILE BACKUP --expected-current-sha256 SHA --dry-run --json`. Use the preview's `before_sha256` and `after_sha256` in the authorized apply command `starbank backup restore FILE BACKUP --expected-current-sha256 LIVE_SHA --expected-backup-sha256 BACKUP_SHA --json`. Review the updated snapshot before retrying a fingerprint conflict. Exit status 6 requires inspection before retrying, including when publication succeeded and the CLI could not deliver its report. Check the destination, backups and any recovery artifacts.

For a selected .SC2Map, use `starbank map inspect FILE --json`. Use `starbank map scan --root PATH --json` only for directories or archives the user selected. Treat declared authors and bank names as metadata hints, and check `incomplete` before treating scan results as exhaustive.
