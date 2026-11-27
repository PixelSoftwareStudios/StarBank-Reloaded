# Native storage verification

This crate supplies the filesystem half of WP-07 and VO-05. It accepts already prepared bytes and a separate create or expected-overwrite intent. The caller owns bank validation, destination authorization and retained backup policy.

## Verified on arm64 macOS

On macOS 14.7.2 build 23H311, local APFS, Rust 1.100.0, the native tests use disposable synthetic files to cover:

- Same-directory exclusive publication and SHA-256 plus identity read-back.
- Existing destination, restrictive length limits, links, ambiguous path syntax, readonly files, and a held destination-directory lease.
- Stale bytes and equal bytes on a different inode.
- Replacement with preserved mode, extended ACL and custom extended attribute, with the original bytes retained at a sibling recovery path.
- A separate native rename after the last expected-fingerprint check. The adapter reports an uncertain publication and retains the external writer's bytes at the sibling path.

### Publication failures and process termination

The APFS qualification added after `c078263` exercised three checkpoints around actual replacement. A pre-publication injected error returned a known rejection, preserved the opened fingerprint, and removed the owned staging file. Injected errors immediately after publication and after directory sync returned `CommitUncertain` and preserved the displaced bytes. An independent exclusive creator won a competing create. A writer that changed the destination between publication and read-back produced an uncertain result and retained the prior bytes.

Two additional APFS tests change the pending file or replace the destination directory at the staging checkpoint. Before the final binding checks, both cases reached the exchange and reported uncertainty. With the checks in place, both return a conflict before publication, preserve the live bytes, and remove the owned pending file. The tests schedule real file writes and directory renames at a deterministic boundary. [Regression tests](../src/native/commit/tests.rs)

The macOS test harness also launched and terminated a child writer with `SIGKILL` after staging, after the native exchange, and after available directory sync. Before each termination an independent writer was refused by the held directory lease. After each termination the target and sibling contained the expected versions and a fresh writer acquired the released lease. The harness uses process pipes and a bounded handshake so the parent can confirm each checkpoint before sending the signal.

These cases cover observable effects of process termination while the operating system remains running. They do not simulate filesystem system-call failures directly, certify power-loss ordering or provide an automatic rollback decision. The helper test is ignored during ordinary discovery and runs directly as the child process in the qualification case.

### Native short-write failure

A child-only `RLIMIT_FSIZE` fixture on macOS APFS used an 8 KiB buffer and ignored `SIGXFSZ`. The control write advanced by 1 KiB before the retry returned `EFBIG`. The public adapter returned the same native error while writing the stage for both Create New and Replace Expected. The parent verified the opened fingerprint and bytes, the absence of the new destination, and removal of the partial stages. A subsequent verified create confirmed that the directory lease was available. The fixture uses small disposable files and leaves the host's file-size limits unchanged. [Apple resource limits](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/setrlimit.2.html) [Apple write errors](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/write.2.html)

This is an observed kernel write failure. Disk exhaustion, device sync errors, and power-loss ordering require separate evidence.

The earlier host probe used the same APFS type to verify the Darwin interfaces. The current tests exercise the product adapter with the configured writer synchronization, while the late-conflict fixture deliberately ignores the cooperative lease.

```sh
tool_bin=$(dirname "$(rustup which --toolchain 1.100.0 rustc)")
export PATH="$tool_bin:$PATH"
cargo fmt --all -- --check
cargo test -p starbank-fs --offline --locked
cargo nextest run -p starbank-fs --offline --locked
cargo clippy -p starbank-fs --all-targets --all-features --offline --locked -- -D warnings
cargo doc -p starbank-fs --no-deps --offline --locked
cargo check -p starbank-fs --offline --locked --target x86_64-pc-windows-msvc
cargo check -p starbank-fs --offline --locked --target wasm32-unknown-unknown
```

WASM cross-checks the adapter that reports unsupported. Linux implements checked snapshots and exclusive creation using `renameat2(RENAME_NOREPLACE)` through rustix. The workspace suite and Linux desktop tests ran on Debian 12 arm64 with Rust 1.100.0. A separate native ext4 probe demonstrated verified exchange of visible metadata and found a gap in verifying existing attributes that Linux hides from an unprivileged caller. [Linux execution evidence](../../../../docs/verification/IMPLEMENTATION.md) · [Native replacement qualification](NATIVE_REPLACEMENT_QUALIFICATION.md)

## Linux staging space exhaustion

At `c8d0b0f`, the native Linux arm64 adapter exercised Create New as UID 65534 with no capabilities. The run used Linux 6.12.76-linuxkit, Rust 1.100.0 and a disposable 64 KiB tmpfs in the cached Debian 12 Rust image at `sha256:bb875b0f8ce726053e259034848c471840e96bfe591fd52556178ae9a05e6126`. The container had no network or host filesystem bind mounts. Repository source and its unchanged locked dependency subset came in on stdin.

The control write accepted 61,440 of 65,537 bytes and then returned `ENOSPC` (28). Create New reported the same native errno from the staging write and rejected publication. The existing bank kept its fingerprint and bytes, the requested destination remained absent, and the partial stage and its storage allocation were released. A subsequent verified Create New confirmed that the directory lease and recovered capacity were available. [Linux write semantics](https://man7.org/linux/man-pages/man2/write.2.html) [Linux tmpfs limits](https://docs.kernel.org/filesystems/tmpfs.html)

To run the opt-in test on Linux, use a dedicated empty tmpfs capped at 1 MiB or less with at least two data blocks available to the test user. The test verifies these conditions before creating fixtures. The 2 GiB build tmpfs was refused at that gate. All 16 ordinary Linux storage tests and the opt-in regression passed. This is separate evidence from the APFS file-size-limit test. Linux replacement, Windows writes and other filesystems retain their existing qualification status. [Regression test](../tests/linux_enospc.rs)

```sh
STARBANK_ENOSPC_TMPFS=/fixture cargo test -p starbank-fs --test linux_enospc \
  --locked --offline -- --ignored --nocapture
```

## Outcomes and remaining gates

The adapter walks directory components without following symlinks and uses directory-relative operations. It takes a nonblocking advisory lease on the destination directory before reading or preparing an overwrite. A fingerprint binds the observed name and parent identity to the file identity, change stamp and SHA-256. Existing-file replacement currently requires local APFS and a supported, writable single-link regular file. macOS overwrites verify the staged owner, group, permissions, ACL and xattrs against the original. File flags and special permission bits are refused. The native publication updates file identity, so timestamps and change tracking are observably distinct after replacement.

The swap preserves a sibling copy even when the caller's retained backup policy is disabled. The receipt names the sibling on success. If a late writer changes the old target, the freshly published bytes may be visible and the displaced writer's version remains as recovery evidence. A post-publish verification or sync failure similarly requires inspection. Callers should not retry an uncertain attempt automatically or treat the sibling as a verified backup. The sibling may have moved during an uncertain exclusive create. No automatic rollback is implemented.

Other filesystems on macOS, Windows native writes, host crash and power-loss ordering, special metadata and native lock/share behavior require separate verification. Advisory locking only coordinates writers that honor it. Changing the directory binding or acting after the final fingerprint check can still affect the outcome. The adapter uses observations and recoverable publication so it does not report those ambiguous effects as ordinary retries. These tests do not certify WP-07, and no formal proof or power-loss claim is made.

## Windows snapshot qualification

The Windows reader accepts ordinary files on a local drive whose opened root reports a local volume GUID path. It walks one component at a time through held directory handles. The open flags block reparse traversal and recall. Observed links, directory targets, alternate data stream syntax, UNC paths, device paths and offline attributes are refused. The leaf is opened for read sharing only, with no write/delete sharing. The reader checks the 128-bit file ID and volume serial number, directory ID, metadata and bounded bytes before issuing a SHA-256 fingerprint. Windows commit intents return an unsupported outcome before opening the destination.

Rust 1.100.0 cross-Clippy compiled the Windows implementation and its test cases. It also passed the cross-target rustdoc build. The Mac host passed the existing native regression cases and the APFS qualification cases above. Windows tests cover local reads, digest and size, identity changes, access under a competing open, hard links, path syntax, and rejected writes. The symbolic-link test requires Windows link-creation permission and runs separately. The Windows MSVC test link could not complete on the Mac host because the MSVC linker was unavailable. A native Windows runner with a local NTFS fixture remains necessary to execute these cases. Generic Wine results cannot qualify NTFS behavior.

```sh
cargo clippy -p starbank-fs --all-targets --offline --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo doc -p starbank-fs --no-deps --offline --locked --target x86_64-pc-windows-msvc

# Execute these commands on Windows with a local NTFS temporary directory.
cargo test -p starbank-fs --locked --test windows_storage
cargo test -p starbank-fs --locked --test windows_storage -- --ignored
```

See the Microsoft [CreateFileW sharing and open flags](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew), [handle-relative names](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile), [volume GUID path contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew), and [file information structures](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex). The handle-relative implementation comes from the pinned [cap-primitives 4.0.3 backend](https://docs.rs/crate/cap-primitives/4.0.3/source/src/windows/fs/create_file_at_w.rs).

The Windows create-only qualification records the local cross-check, prospective outcome model and remaining native NTFS cases. [Windows create-only qualification](WINDOWS_CREATE_QUALIFICATION.md)
