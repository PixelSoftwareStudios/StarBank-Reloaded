# Windows create-only save qualification

Baseline `e2cf035`, WP-07. Existing Windows save intents remain unsupported. Create-only activation needs a native local NTFS run. Replacement remains a separate capability and remains unavailable.

## Checked environment

- The local host is macOS 14.7.2 on arm64. A read-only VirtualBox 7.2.22 inventory outside the restricted socket sandbox reported zero registered guests and zero running guests. Docker reports a Linux arm64 backend. No alternate VM manager was available through the installed command-line tools.
- Wine is installed. It cannot establish native Windows NTFS behavior. No external runner, account token, VM image, or guest data was used.
- The exact repository revision passed all 20 macOS storage cases with nextest 0.9.151. Nextest skips the subprocess worker entry during discovery, and the crash test invokes it at each of three checkpoints.
- The exact repository revision passed Windows x64 MSVC cross-Clippy for all `starbank-fs` targets with warnings denied, using Rust 1.100.0. This compile gate includes the existing read-only storage integration tests.
- The accompanying no-effect outcome model ran three fixture tests on macOS and compiled as Windows x64 test metadata using Rust 1.100.0. It requires a verified parent and the staged file ID plus its SHA-256 at the requested name. A documented collision must still have the owned stage and a distinct occupant on the same volume. Mismatched data, same-byte different identity, an unverified parent or an ambiguous call result keep the transaction uncertain.

The Windows MSVC test link failed locally because `link.exe` was unavailable. No Windows runtime tests executed. The model exercises the adapter's prospective classification rule independently from the operating system and does not model NTFS.

## Candidate for native testing

1. Open and hold the authorised parent using the current component-by-component no-follow policy. Check the filesystem through the actual parent handle and require local NTFS plus a stable volume and directory file ID. Recheck the path's bound parent before reporting success. [Microsoft volume information by handle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew)
2. Create an unpredictable sibling exclusively under a private staging DACL. Verify the stage is a single-link ordinary file and capture its file ID. Prepare the complete bytes, call the supported file flush operation, and retain the handle needed for publication. Prove that another ordinary user with access to the parent cannot read an unpublished stage. [Microsoft CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew) [Microsoft FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
3. First qualify handle-based `SetFileInformationByHandle(FileRenameInfo)` with `ReplaceIfExists = FALSE`, the held parent as `RootDirectory` and a single destination filename. The Windows documentation describes the relative parent form. Test it in the actual NTFS environment and verify that a concurrent occupant is preserved. Avoid a path-only fallback or cross-volume copy mode while this binding is unresolved. [Microsoft FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info) [Microsoft SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)
4. Capture name collision and other native return codes immediately. Reopen under the held scope and verify the stage identity and digest at the requested name, stage name absence, and the intended file security. Separately qualify available unprivileged namespace durability behavior. If the call or verification leaves an ambiguous result, retain any staged or published files for explicit inspection. [Microsoft FileIdInfo](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)

## Handle-relative rename probe

The Windows-only unit test exercises the proposed kernel operation on synthetic files in a fresh temporary directory through the current local path resolver. It requires the opened directory handle to report NTFS. It flushes a complete, uniquely scoped stage, places competing names after staging, then calls `SetFileInformationByHandle(FileRenameInfo)` with an inline UTF-16 leaf, the held parent handle, and replacement disabled. It checks ordinary and case-alias collisions against the original occupant identity and bytes, and verifies the stage file ID, unchanged parent, digest and missing stage name after publication. The test records the collision codes and observed identities. [Probe source](../src/windows/rename_qualification.rs)

Run this opt-in test on an actual Windows host under an ordinary user with a case-insensitive local NTFS temporary directory. Record the Windows build and test output.

```powershell
cargo test --locked -p starbank-fs --lib local_ntfs_handle_relative_create_only_publication -- --ignored --nocapture
```

The probe covers the rename and read-back boundary. The synthetic stage inherits directory security. Private staging DACLs, the namespace durability available to an unprivileged process, externally changing directory scope, checkpoint termination, and production failure classification still require qualification. The recorded macOS authoring pass compiled the probe with strict Windows MSVC cross-Clippy. Local PE linking with LLVM LLD 22.1.8 failed for absent Windows SDK import libraries, including `kernel32.lib` and `ntdll.lib`. The probe remains unexecuted on native Windows. The `windows-2025` CI job selects it explicitly through nextest and fails if no test matches. Hosted execution remains pending.

## Native acceptance

| Fixture | Expected observation |
|---|---|
| Ordinary file on local NTFS | Complete staged bytes at the new name, no stage name, matching file ID and volume, private verified permissions, and successful supported flushes. |
| Target exists before publication | Existing target retains its independent identity and bytes. An explicit collision is classified only from the qualified failure result and valid observations. |
| Target appears after staging | A cooperating fixture creates the exact target immediately before rename. It survives byte-for-byte. Repeat with a differently cased name on a case-insensitive directory. |
| Unsafe paths and volumes | Refuse before staging on a reparse ancestor, alternate stream, remote path, and unqualified filesystem. A destination placed on a different volume never receives a copied file. |
| Late directory rebind or access change | Fail safely or preserve uncertain evidence. Recheck the owning parent by handle. Avoid guessing that an I/O error means no effect. |
| Commit checkpoint termination | Stop an isolated child before publication, immediately after the kernel call, and after available sync. Inspect both names and digests from an independent process. |
| Failed read-back | A changed or unreadable destination produces an uncertain result, and any owned artifact stays recoverable. |

Record exact Windows build, NTFS volume type, toolchain, token capabilities, affected file IDs, sizes, digests, native error codes and checkpoint results for synthetic inputs. Describe crash-process behavior independently from OS or power interruption. Keep existing default refusal until the native acceptance run passes.

## Reproduce the local evidence

```sh
rustc --test --edition 2024 -D warnings create_outcome_model.rs -o create-outcome-model
./create-outcome-model
rustc --test --edition 2024 -D warnings --target x86_64-pc-windows-msvc \
  --emit=metadata create_outcome_model.rs

# Run in a clean archive of the repository baseline with cached dependencies.
cargo clippy -p starbank-fs --all-targets --locked --offline \
  --target x86_64-pc-windows-msvc -- -D warnings
```
