# Backup catalog verification

This slice implements the storage-independent part of REQ-BACKUP-01 and REQ-BACKUP-02. Backup filenames use an explicitly supplied UTC timestamp and preserve the bank basename. The storage port must publish snapshots exclusively, verify exact bytes, and persist their metadata before the catalog allows a live bank replacement. The catalog records SHA-256 and length and keeps a verified copy of the current target before restore. Only a verified protected overwrite satisfies the session's first-overwrite policy. The caller selects the managed directory, loads the persisted catalog, supplies the authorized scope and handles user confirmation.

On macOS arm64 with Rust 1.100.0, these checks passed with original synthetic data:

- `cargo fmt --all -- --check`
- `cargo test -p starbank-backup --locked --offline` and `cargo nextest run -p starbank-backup --locked --offline` each passed 12 tests.
- `cargo clippy -p starbank-backup --locked --offline --all-targets -- -D warnings`
- `cargo doc -p starbank-backup --locked --offline --no-deps`
- `cargo check -p starbank-backup --locked --offline` for `wasm32-unknown-unknown` and `x86_64-pc-windows-msvc`. These are domain compile checks.
- `cargo run -p starbank-source-check --locked --offline` checked the workspace source layout.

A disposable APFS probe compiled the backup and native filesystem crates together, with a test adapter that saved metadata through the native exclusive-create path. It preserved a preexisting backup at the same timestamp, saved and restored exact XML bytes including CRLF, and preserved the reversal snapshot. A preexisting metadata sidecar stopped the overwrite, kept its existing content and retained the newly created orphan snapshot for recovery. The probe used synthetic files under `/private/tmp`.

The native probe covers ordinary APFS files. It leaves open the production catalog adapter and its restart behavior, GUI and batch integration, native overwrite and restore beyond qualified filesystems, and crash and power-loss tests. See the storage crate for native commit boundaries.
