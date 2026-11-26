# Native storage verification

This crate supplies the filesystem half of WP-07 and VO-05. It accepts already prepared bytes and a separate create or expected-overwrite intent. The caller owns bank validation, destination authorization and retained backup policy.

## Verified on arm64 macOS

On macOS 14.7.2 build 23H311, local APFS, Rust 1.100.0, the native tests use disposable synthetic files to cover:

- Same-directory exclusive publication and SHA-256 plus identity read-back.
- Existing destination, restrictive length limits, links, ambiguous path syntax, readonly files, and a held destination-directory lease.
- Stale bytes and equal bytes on a different inode.
- Replacement with preserved mode, extended ACL and custom extended attribute, with the original bytes retained at a sibling recovery path.
- A separate native rename after the last expected-fingerprint check. The adapter reports an uncertain publication and retains the external writer's bytes at the sibling path.

The earlier host probe used the same APFS type to verify the Darwin interfaces. The current tests exercise the product adapter with the configured writer synchronization, while the late-conflict fixture deliberately ignores the cooperative lease.

```sh
tool_bin=$(dirname "$(rustup which --toolchain 1.100.0 rustc)")
export PATH="$tool_bin:$PATH"
cargo fmt --all -- --check
cargo test -p starbank-fs --offline --locked
cargo clippy -p starbank-fs --all-targets --all-features --offline --locked -- -D warnings
cargo doc -p starbank-fs --no-deps --offline --locked
cargo check -p starbank-fs --offline --locked --target x86_64-pc-windows-msvc
cargo check -p starbank-fs --offline --locked --target wasm32-unknown-unknown
```

Windows and WASM cross-check the adapter that reports unsupported. Linux implements checked snapshots and exclusive creation using `renameat2(RENAME_NOREPLACE)` through rustix. The Linux test target was checked locally with Rust 1.99.0 and `--ignore-rust-version` because the installed 1.100.0 compiler lacks the Linux standard library. Execution on Linux and verification with the pinned compiler are pending.

## Outcomes and remaining gates

The adapter walks directory components without following symlinks and uses directory-relative operations. It takes a nonblocking advisory lease on the destination directory before reading or preparing an overwrite. A fingerprint binds the observed name and parent identity to the file identity, change stamp and SHA-256. Existing-file replacement currently requires local APFS and a supported, writable single-link regular file. macOS overwrites verify the staged owner, group, permissions, ACL and xattrs against the original. File flags and special permission bits are refused. The native publication updates file identity, so timestamps and change tracking are observably distinct after replacement.

The swap preserves a sibling copy even when the caller's retained backup policy is disabled. The receipt names the sibling on success. If a late writer changes the old target, the freshly published bytes may be visible and the displaced writer's version remains as recovery evidence. A post-publish verification or sync failure similarly requires inspection. Callers should not retry an uncertain attempt automatically or treat the sibling as a verified backup. The sibling may have moved during an uncertain exclusive create. No automatic rollback is implemented.

Other filesystems on macOS, Windows native storage, crash and power-loss ordering, special metadata and native lock/share behavior require separate verification. Advisory locking only coordinates writers that honor it. Changing the directory binding or acting after the final fingerprint check can still affect the outcome. The adapter uses observations and recoverable publication so it does not report those ambiguous effects as ordinary retries. These tests do not certify WP-07, and no formal proof or power-loss claim is made.
