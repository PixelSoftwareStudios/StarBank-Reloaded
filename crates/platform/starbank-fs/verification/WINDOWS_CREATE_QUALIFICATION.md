# Windows create-only save qualification

Baseline `e2cf035`, WP-07 and REQ-SAVE-02. Windows storage is read-only. Create-only activation needs native local NTFS qualification. Replacement requires separate qualification.

## Checked environment

- The local host is macOS 14.7.2 on arm64. A read-only VirtualBox 7.2.22 inventory outside the restricted socket sandbox reported zero registered guests and zero running guests. Docker reports a Linux arm64 backend. No alternate VM manager was available through the installed command-line tools.
- The baseline local checks used only the macOS host and cached toolchain. Native NTFS behavior requires Windows.
- The exact repository revision passed all 20 macOS storage cases with nextest 0.9.151. Nextest skips the subprocess worker entry during discovery, and the crash test invokes it at each of three checkpoints.
- The exact repository revision passed Windows x64 MSVC cross-Clippy for all `starbank-fs` targets with warnings denied, using Rust 1.100.0. This compile gate includes the existing read-only storage integration tests.
- The accompanying no-effect outcome model ran three fixture tests on macOS and compiled as Windows x64 test metadata using Rust 1.100.0. It requires a verified parent and the staged file ID plus its SHA-256 at the requested name. A documented collision must still have the owned stage and a distinct occupant on the same volume. Mismatched data, same-byte different identity, an unverified parent or an ambiguous call result keep the transaction uncertain.

The baseline Windows MSVC test link failed locally because `link.exe` was unavailable. The outcome model tests the adapter's prospective classification rule independently from filesystem execution.

## Hosted Win32 attempt

On November 27, 2026, run `45984131228` at `4143952` reached the first collision in the original `SetFileInformationByHandle(FileRenameInfo)` probe on Windows Server 2025 x64. It returned `ERROR_INVALID_PARAMETER` (87) and failed the required collision assertion. The case-alias, post-collision integrity and positive publication checks were pending when the test stopped. The failure leaves the cause of error 87 unresolved. [Hosted Rust native job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45984131228/job/140455268612) · [Win32 rename structure](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)

## Candidate for native testing

1. Open and hold the authorised parent using the current component-by-component no-follow policy. Check the filesystem through the actual parent handle and require local NTFS plus a stable volume and directory file ID. Recheck the path's bound parent before reporting success. [Microsoft volume information by handle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew)
2. Create an unpredictable sibling exclusively under a private staging DACL. Verify the stage is a single-link ordinary file and capture its file ID. Prepare the complete bytes, call the supported file flush operation, and retain the handle needed for publication. Prove that another ordinary user with access to the parent cannot read an unpublished stage. [Microsoft CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew) [Microsoft FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
3. Qualify `NtSetInformationFile(FileRenameInformation)` with `ReplaceIfExists` false, the held destination parent as `RootDirectory` and a single destination leaf. Verify explicit destination binding with distinct directories on one volume, then verify sibling publication and refusal when a competing name is present. [Microsoft FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information) · [Microsoft NtSetInformationFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntsetinformationfile)
4. Capture name collision and other native return codes immediately. Reopen under the held scope and verify the stage identity and digest at the requested name, stage name absence, and the intended file security. Separately qualify available unprivileged namespace durability behavior. If the call or verification leaves an ambiguous result, retain any staged or published files for explicit inspection. [Microsoft FileIdInfo](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)

## Handle-relative rename probe

The Windows-only unit test uses synthetic files in a fresh temporary directory. It requires the held directory handle to report local NTFS, creates each stage exclusively relative to its held source parent, and flushes the complete bytes before calling the pinned `windows-sys` binding. The request supplies the held destination parent, a bounded inline UTF-16 leaf and replacement disabled. The stage handle uses synchronous I/O, so the direct completed `NTSTATUS` is authoritative. [Probe source](../src/windows/rename_qualification.rs) · [Microsoft I/O status block](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_io_status_block)

The explicit-root control uses distinct source and target directory IDs on one volume and starts with the destination leaf absent in both. It requires `STATUS_SUCCESS` (0), verifies the original stage file ID and SHA-256 under the held target directory, and checks that both the stage name and the destination leaf are absent in the source directory. The sibling cases place each competitor after staging and require `STATUS_OBJECT_NAME_COLLISION` (`0xc0000035`) for exact and case-alias names, while checking that the occupant and stage retain their identities and bytes. The final sibling publication requires `STATUS_SUCCESS` and verifies the destination directory binding, file ID and digest and the former stage name's absence. [Microsoft NTSTATUS values](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/596a1078-e883-4972-9bbc-49e60bebca55)

Run this opt-in test on an actual Windows host under an ordinary user with a case-insensitive local NTFS temporary directory. Record the Windows build and test output.

```powershell
cargo test --locked -p starbank-fs --lib local_ntfs_handle_relative_create_only_publication -- --ignored --nocapture
```

The `windows-2025` CI job selects the probe explicitly through nextest and fails if no test matches.

## Hosted NTFS probe

On November 27, 2026, run `45997318578` at `e4385da` passed the `NtSetInformationFile(FileRenameInformation)` probe on Windows Server 2025 x64 build 10.0.26100 with runner image `windows-2025-vs2026 20261123.321.1`. The distinct-root control on one local NTFS volume and the final sibling publication each returned `STATUS_SUCCESS` (`0x00000000`). Exact-name and case-alias competitors each returned `STATUS_OBJECT_NAME_COLLISION` (`0xc0000035`) with their identities and contents preserved. Both successful publications retained the stage file ID and the synthetic 38-byte payload SHA-256 `ea59fad9139058244a0f8cd3c6c1b8a51152bb72ca32254134280b64d5a74ee1` at the destination, with the old stage name absent. [Hosted Windows Rust job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/45997318578/job/140499862286)

This result qualifies the sibling and distinct-root rename and read-back probe on the tested runner. The stage inherits directory security. Private staging DACLs, unprivileged namespace durability, late directory changes, checkpoint termination and production failure classification remain open. Windows storage remains read-only.

## Private stage descriptor probe

The existing opt-in NTFS test now includes a synthetic creation-time security probe for REQ-SAVE-02. It creates a fixture directory with an inheritable Everyone read ACE and verifies that an ordinary inherited control receives that ACE. The stage instead receives an explicit TokenUser owner, TokenPrimaryGroup group and protected owner-only DACL during exclusive, handle-relative `NtCreateFile`. Before writing any bytes, the probe checks the resulting owner, group, individual ACEs, scoped descriptor flags and ordinary single-link file attributes. It checks them again through the held handle and a fresh destination handle after publication, together with the original file ID and payload digest. Account SIDs are redacted from observations. [Security probe](../src/windows/rename_qualification/security_qualification.rs)

Local Windows x64 MSVC compilation and all-target Clippy with warnings denied passed on the macOS host using Rust 1.100.0. Native Windows execution is pending. The existing hosted NTFS test selection includes this extension. This probe checks descriptor metadata; a distinct-user access denial, SACL inspection, post-publication sync, checkpoint termination and production outcome classification remain separate obligations. Windows bank writes remain disabled.

### First hosted descriptor attempt

Hosted run `46016246440` at `7d960c4` passed all 243 default Windows tests and the earlier rename/collision assertions, then the new security fixture returned `STATUS_INVALID_PARAMETER` (`0xc000000d`) from `NtCreateFile`. The original diagnostic did not identify which creation call failed, so private staging was not qualified. The token was elevated and its default owner differed from TokenUser; this run cannot prove ordinary-user behavior. [Windows job](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46016246440/job/140564004999)

The corrected fixture uses documented directory-compatible creation options and minimal directory access. Reparse/recall guards remain on regular stage/control files; directory creation is exclusive under the held synthetic temporary parent. Creation errors now include the fixture leaf and exact options. Native execution of the correction remains pending. This fixes an identified parameter-contract violation without claiming to have isolated the original kernel rejection. [NtCreateFile directory parameters](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile#parameters)

## Held-stage read-back probe

The opt-in NTFS test also checks read-back while the original restrictive stage handle remains open. Its independent verifier requests read access and shares read, write and delete to accommodate the existing stage handle; the stage itself still denies competing write and delete access. The fixture requires actual sharing-violation responses for incompatible opens, checks the held file's ID, length and SHA-256, then releases the handles and confirms the competing access requests can succeed without writing or deleting. An ordinary snapshot read verifies the final published bytes afterward. This case uses inherited fixture permissions; creation-time private security is checked separately. [Held-stage probe](../src/windows/rename_qualification/readback_qualification.rs)

Pinned Windows x64 MSVC all-target Clippy, formatting and the repository source gate passed locally. Native execution is pending. The test does not change the production reader's sharing policy or enable Windows writes.

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
