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

This is an observed kernel write failure. The capacity test below supplies separate disk-exhaustion evidence. Device sync errors and power-loss ordering remain open.

### APFS capacity exhaustion

On macOS 14.7.2 arm64 with Rust 1.100.0, a headless 128 MiB sparse APFS image reported 134,176,768 bytes of filesystem capacity. The test wrote and synced 124,379,136 bytes of filler, leaving 8,388,608 bytes available. A native 16 MiB control write returned `ENOSPC` (28), with zero payload bytes accepted. Both Create New and Replace Expected then returned `RejectedUnchanged` with the same errno from `write pending file`.

The original bank retained its complete fingerprint, bytes, mode and custom extended attribute. No new destination or owned pending file remained. Removing the filler recovered capacity and a small verified Create New succeeded, proving the lease was released. The guarded test also rejected an ordinary unmounted directory before creating fixtures. The harness detached its own image and removed the successful run's scratch files. Failed runs retain their detached image for diagnosis; failed mount inspection or detachment retains the fixture without deleting its backing file. [Test](../tests/macos_enospc.rs) · [Harness](macos-enospc.sh)

```sh
sh crates/platform/starbank-fs/verification/macos-enospc.sh
```

The harness requires macOS, the repository Rust toolchain, cargo-nextest and at least 1 GiB of free host space. It creates a private image under `/private/tmp`, verifies a distinct local APFS mount capped at 128 MiB, and only fills that fixture. The test stays ignored in ordinary runs; the harness requires one nonempty ignored-test run. The integrated run passed through cargo-nextest 0.9.151 (CI pins 0.9.152), and the normal filesystem suite passed 29 tests with three ignored qualification helpers. Formatting, strict Clippy, ShellCheck, the source gate and workflow validation passed. The hosted harness passed at `e4385da`. At `e6938ce`, it returned an unlabeled errno 28 after the default macOS tests passed; that failure's precise phase remains unknown. [Passing run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45997318578) · [Failing macOS job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45998682365/job/140504475028)

The revised fixture labels I/O phases and runs two cases sequentially on the same disposable image, removing each case directory before starting the next. The second case deliberately exhausts filler writes and requires bounded calibration by truncating only the fixture's synced filler. On macOS 14.7.2 arm64, this reached actual filler-write `ENOSPC` after syncing 128,974,848 bytes. Truncating to 120,586,240 bytes left 12,181,504 reported available bytes. Both cases still required the independent 16 MiB write failure, both typed staging refusals, unchanged original bytes/fingerprint/mode/xattr, no pending files, and a verified retry after removing the filler. The single nextest test passed in 5.115 seconds, strict Clippy and formatting passed, and the image detached and cleaned up. This exercises the new calibration path; it does not identify the earlier hosted failure's phase.

The hosted macOS job at `4ffb6c40d2bc746f1ab3c79a4f1b14c5f96b45a4` passed both cases in 3.082 seconds. The reported-threshold case left 8,388,608 bytes available without calibration. In the forced case, a filler write returned `ENOSPC` and the first calibration attempt left 12,181,504 bytes available. Each native control accepted zero bytes of its 16 MiB payload. Both staging intents returned errno 28, the original fingerprint and metadata were preserved, and retry succeeded after filler removal. All 11 jobs passed at this revision. [Hosted macOS job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46000630338/job/140511049779) · [Workflow run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46000630338)

The evidence covers insufficient APFS staging capacity on synthetic files on a disposable image. Zero-free-block cleanup, device failure, failed synchronization and physical power loss remain open.

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

Hosted run `46033460924` at `bef77ce` passed both APFS disk-exhaustion cases, their native write controls and explicit retries, then failed cleanup because ordinary `hdiutil detach` reported the image was busy. The cause of the retained mount was not established. The fixture correctly retained its backing image rather than treating the test passes as successful cleanup. [macOS result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46033460924/job/140622829301)

The cleanup harness now permits six bounded ordinary detach attempts. Before each detach it checks the exact image path, original attach device and current mount device against fresh image and mount inventories. Partial disappearance only permits waiting; ownership mismatch or malformed evidence refuses detach. Both mount and image attachment must be absent before deletion. Failed tests still retain their evidence and failure status after detachment. No forced detach or helper termination is used. Shell syntax and 20 synthetic ownership/retry cases passed locally; native execution of this change is pending. [Ownership cases](test_macos_enospc_state.py)

Run `46035306785` at `373c332` passed ordinary detach on its first attempt, confirming the new cleanup harness on that runner. The threshold exhaustion case passed, but the separate UntilEnospc case failed qualification: after a raw control reported ENOSPC and was deleted, the writer successfully committed the full 16 MiB create. This is a successful write after capacity became available, not evidence of a rejected staging write. The replacement attempt was not reached. Retaining controls and calibrating allocated copies are being evaluated before rerunning that case; the failed result is preserved. [macOS result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46035306785/job/140629089146)

The revised fixture precreates its raw control while space is available, retains its open handle and partial allocation through both required target checks, and independently packs verified synthetic copies before each check. Every successful calibration output is read back against its receipt and kept; those successes are allocation evidence, not simulated failures. Attempts, requested bytes and retained verified bytes are bounded by the checked image capacity. A measured owned metadata reserve is released immediately before each required transaction, and the resulting reported space is logged. Both targets must still return native ENOSPC during `write pending file` and preserve the original bytes, full fingerprint, mode and xattr. Failed case directories remain on the retained image; only completely successful cases are explicitly removed after verified retry. Formatting, owning-test Clippy and source checks passed locally. Native execution of this revision remains pending. [Calibration](../tests/macos_enospc/calibration.rs)

The adapter walks directory components without following symlinks and uses directory-relative operations. It takes a nonblocking advisory lease on the destination directory before reading or preparing an overwrite. A fingerprint binds the observed name and parent identity to the file identity, change stamp and SHA-256. Existing-file replacement currently requires local APFS and a supported, writable single-link regular file. macOS overwrites verify the staged owner, group, permissions, ACL and xattrs against the original. File flags and special permission bits are refused. The native publication updates file identity, so timestamps and change tracking are observably distinct after replacement.

The swap preserves a sibling copy even when the caller's retained backup policy is disabled. The receipt names the sibling on success. If a late writer changes the old target, the freshly published bytes may be visible and the displaced writer's version remains as recovery evidence. A post-publish verification or sync failure similarly requires inspection. Callers should not retry an uncertain attempt automatically or treat the sibling as a verified backup. The sibling may have moved during an uncertain exclusive create. No automatic rollback is implemented.

Other filesystems on macOS, Windows existing-file replacement, host crash and power-loss ordering, special metadata and additional native lock/share behavior require separate verification. Windows local NTFS create-only transactions have their own native qualification record. Advisory locking only coordinates writers that honor it. Changing the directory binding or acting after the final fingerprint check can still affect the outcome. The adapter uses observations and recoverable publication so it does not report those ambiguous effects as ordinary retries. These tests do not certify WP-07, and no formal proof or power-loss claim is made.

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

Windows inventory authority now compares the volume serial and 128-bit directory ID through the same audited directory scope. Creation timestamps cannot authorize a replacement directory. Timestamp-only manual-folder records from older builds require explicit folder re-selection or an explicit switch to auto-detect; passive startup and refresh preserve the old bytes. The outer record version and Unix device/inode representation stay unchanged. Windows-only regressions force equal creation times on different directories and exercise saved-record recovery and restart. Both tests passed natively among the 97 Windows desktop tests at `836efb7`. [Authority and recovery tests](../../../../apps/starbank-desktop/src-tauri/src/inventory/root_recovery_tests.rs) · [Native test run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46007575467/job/140534548278)
