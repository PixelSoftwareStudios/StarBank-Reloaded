# Windows create-only save qualification

WP-07 and REQ-SAVE-02. Windows enables create-only persistence in qualified local NTFS directories below the drive root. Replacement remains unsupported. The entries below record qualification chronologically from baseline `e2cf035`; disabled states in earlier entries apply to those revisions.

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

The correction at `bae4984` passed directory creation and the actual inherited Everyone read ACE checks in native run `46017448068`. It then stopped at an overly broad fixture assumption that every inherited DACL sets `SE_DACL_AUTO_INHERITED`. The fixture now logs those descriptor bits while retaining the actual inherited-ACE, owner, group and unprotected-control assertions. All exact private-DACL checks remain in force. Private-stage execution had not been reached in this run. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46017448068/job/140568094400) · [Descriptor flags](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-descriptor-control) · [Inherited ACE flag](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-ace_header)

## Held-stage read-back probe

The opt-in NTFS test also checks read-back while the original restrictive stage handle remains open. Its independent verifier requests read access and shares read, write and delete to accommodate the existing stage handle; the stage itself still denies competing write and delete access. The fixture requires actual sharing-violation responses for incompatible opens, checks the held file's ID, length and SHA-256, then releases the handles and confirms the competing access requests can succeed without writing or deleting. An ordinary snapshot read verifies the final published bytes afterward. This case uses inherited fixture permissions; creation-time private security is checked separately. [Held-stage probe](../src/windows/rename_qualification/readback_qualification.rs)

Pinned Windows x64 MSVC all-target Clippy, formatting and the repository source gate passed locally. Native execution is pending. The test does not change the production reader's sharing policy or enable Windows writes.

## Hosted private-stage and held-readback result

Run `46018191073` at `19b95dc` passed the expanded NTFS probe on native Windows, plus all 244 default tests and five transport probes. Held read-back retained the same 38-byte payload and digest; ordinary-reader, competing-write and competing-delete opens each returned Win32 sharing violation 32. The inherited control had two ACEs and scoped control `0x8004`. The private stage retained its explicit TokenUser owner, token primary group, owner-only DACL, scoped control `0x9004` and archive attribute `0x20` before writing and through publication. The token was elevated, so ordinary-user and distinct-user qualifications remain open. [Native Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46018191073/job/140570653318)

## Flush capability probe

A separate synthetic NTFS fixture now measures `FlushFileBuffers` and normal `NtFlushBuffersFileEx` calls before and after exclusive rename. It records requested access/options/sharing, observed granted access and mode, direct native status and immediate Win32 errors. Both file APIs must succeed. Directory access-denied results are reported explicitly; other unexpected results fail the probe for review. A separately opened writable directory is matched to the held scope before use. The fixture retains collision, file-ID, digest and name-binding checks. [Flush probe](../src/windows/rename_qualification/flush_qualification.rs)

The stage helper already performs `sync_all` before these measured calls, so they establish additional call acceptance rather than an independent writeback experiment. The existing token-elevation evidence remains relevant; an elevated hosted token cannot prove ordinary-user behavior. No volume handles or privilege changes are requested. These observations do not establish namespace or power-loss durability. Local Windows-target strict Clippy, pinned formatting and the source gate passed; native execution is pending.

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

## Private-stage implementation and reduced-token qualification

The private-stage primitive is now shared by the native qualifier and the candidate writer. It selects the effective thread token, falling back to the process token only for `ERROR_NO_TOKEN`, creates an empty file with an explicit protected owner-only descriptor, and verifies owner, group, ACEs and descriptor flags before payload bytes. Creation accepts a bounded single UTF-16 leaf and uses the held parent. Native NTSTATUS remains available in the typed I/O cause. Windows runtime writes remain disabled while these modules are wired for test qualification. [Security boundary](../src/windows/stage_security.rs)

The revised qualifier prepares only its empty, test-owned NTFS temporary directory with an explicit user ACL. A dedicated thread disables privileges other than optional traverse and disables Administrators membership; it independently reopens the fixture. A separate same-user World-restricted thread must read the inherited control and receive access denied on the private stage with equivalent read sharing. The production token selector must report `TokenImpersonation` inside the creator context. The qualifier also rejects the inherited control's security, preserves an occupied leaf, and checks the private descriptor through publication. The flush probe runs under the reduced creator as well. Both threads verify effective identity and restore thread security on exit. This is not a different-account or UAC-token test.

Pinned Windows all-target Clippy, formatting and the source gate passed locally. These reduced-token and shared-primitive paths have not yet run natively. The earlier elevated-token observations remain valid for `1a574e1`; its full hosted matrix passed all 11 jobs, including the modern Windows cache-ID regression and both flush APIs. [Hosted matrix](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46018857958)

## Create-only transaction candidate

A single candidate transaction now composes the shared private-stage primitive, held NTFS scope, no-replace rename, available file/directory flushes and read-back. Runtime `commit` remains unsupported on Windows while the candidate is compiled into native unit-test qualification. The locked `getrandom` dependency already used by the Unix writer also supplies Windows stage names; no version or registry package changed.

The candidate opens a writable parent from held ancestry and verifies its ID, rejects case-sensitive directories and reserved Win32 names before staging, and preserves native UTF-16 leaf units. A thread-affine, nonblocking Global mutex keyed by volume and directory ID coordinates cooperating writers. It neither enables privileges nor writes a lock file. An inaccessible or abandoned lease rejects before staging; unrelated external writers remain outside that coordination.

Before publication, failed preparation disposes only the owned file handle. A cleanup failure retains its recorded private location as uncertain. The stage is retained before entering the rename call. Only a documented collision with intact stage and independent occupant evidence permits unchanged rejection. After successful publication, failures retain the destination as the recovery hint. Verification checks the private descriptor, bytes, digest, IDs, disappeared stage name and parent binding while the writer is held; it then closes the writer, captures finalized metadata with a restrictive reader and reopens the terminal leaf before returning a receipt. No namespace or power-loss durability claim follows from flush success.

One opt-in native case executes 20 synthetic scenarios under the verified reduced-token actor, including empty/nonempty publication, preparation and post-publication cut points, exact/case-alias/equal-byte collisions, a read-only metadata change, locked final read-back, same-byte different-ID replacement, bounds and lease contention. The contender thread acquires its own reduced token. These 20 scenarios passed in the hosted run recorded below. Pinned Windows-target strict Clippy, formatting and the source gate passed locally. Actual sync-failure qualification remains pending. [Candidate](../src/windows/create.rs) · [Native cases](../src/windows/create/tests.rs)

```powershell
cargo nextest run --locked -p starbank-fs --lib --run-ignored only --no-tests fail --success-output immediate -E 'test(=windows::create::tests::local_ntfs_create_candidate_transactions)'
```

The Windows CI job selects this case explicitly after the existing NTFS probe. Public Windows writes and the desktop create-copy capability remain disabled until qualification completes.

### First candidate execution

Run `46027037190` at `5a057d1` passed all 249 default Windows tests and the legacy reduced-token probe, then the candidate's first nominal create rejected before staging. `ReOpenFile` returned Win32 error 87 on the cap-primitives-opened parent; subsequent candidate scenarios had not run. The other ten hosted jobs passed. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46027037190/job/140600985960)

The correction opens nested writable parents by the already validated leaf from their held immediate ancestor. Concrete `FILE_GENERIC_WRITE` rights avoid cap-primitives' reserved option for the generic-write bit. No-follow, no-recall, directory identity and case-policy checks remain. The ancestor-less drive-root branch retains `ReOpenFile` on its original Win32-opened handle; it is still runtime-unqualified and rejects before staging if unavailable.

The staging-name policy uses a three-digit extension different from the destination's extension. This keeps both its random long name and automatic DOS 8.3 extension separate from the requested destination, including a destination that resembles the staging prefix. No random-collision retry is introduced. A Windows unit regression checks the staging-looking, case-varied and short-name-shaped inputs.

Run `46029596345` at `27e4a53` passed all 11 jobs. The Windows native job passed 250 default tests, the reduced-token NTFS probe, the candidate's 20 transaction scenarios and five single-instance tests. This confirms nested writable-parent opening on that runner. Actual volume short-name generation, drive-root publication and power-loss recovery were not exercised. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46029596345/job/140609735573)

### Process-interruption cases

The same opt-in candidate test now also starts an owned child at each of three checkpoints: staged, published and directory-synced. A nonce and marker bind the child to the parent's held directory identity. The child receives its START byte only after assignment to a non-inherited kill-on-close job. Each actor enters its own reduced-token context. The parent verifies a live writer causes a no-stage busy rejection, terminates and reaps the child, then verifies the exact retained artifact's identity, bytes, digest, attributes and private security. An open-existing mutex handle retains abandoned state so the next attempt must reject without cleaning or replaying the artifact. Child and pipe-reader waits are bounded. [Process cases](../src/windows/create/process_tests.rs)

Windows-target strict Clippy, formatting, the source gate and documentation validation passed locally for these additions. Run `46031672647` at `769c3f4` passed the Windows candidate wrapper in 0.830 seconds. Its log records all three cuts: Staged, Published and DirectorySynced, each with live Busy rejection, an independently killed and reaped child, retained identity/digest/security and Abandoned rejection. The wrapper runs the earlier 20 scenarios before the three process cuts. The same native job passed 250 default tests, the reduced-token NTFS probe and five single-instance tests. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46031672647/job/140616771981)

These cases test process death while Windows remains running; they provide no OS-crash or physical power-loss evidence. Job-object bindings are enabled only for Windows test dependencies; the public writer and create-copy capability remain disabled while disk-full qualification is pending.

### Bounded disk-full fixture

The owning crate includes a hosted-only PowerShell fixture that creates a new fixed 128 MiB VHD under an exclusive runner-temporary directory. It reserves host headroom first, derives the disk and format target from that exact mounted image, rejects boot/system or unexpected devices and partitions, and verifies the image is detached before deleting its backing file. It never selects or formats an existing physical disk. [Provisioning](windows-enospc.ps1)

The ignored native test checks the mounted NTFS volume's GUID, random label and marker, capacity and absence of quota-based free-space reductions before allocating its bounded filler through held handles. Under the reduced-token actor, an independent write control must return a native disk-full error and retain its previously synced prefix. The actual transaction must fail during staging write without publication, preserve the original fingerprint and bytes, and either clean only its own stage or report its actual retained private stage as uncertain. It records observed EOF without assuming a failed write leaves a short file. After disposing only the fixture filler, a new explicit transaction must return a verified receipt and exact read-back. [Native case](../src/windows/create/stage/enospc.rs)

Local formatting, Windows-target strict Clippy, the source gate and documentation validation are the integration gates. Native execution and Storage provider behavior remain pending; neither a static script check nor cross-compilation establishes disk-full behavior. The Windows native CI job runs the fixture explicitly. Public Windows writes remain disabled.

Run `46033460924` at `bef77ce` successfully provisioned the fixed 134,217,728-byte VHD and its 133,099,520-byte NTFS volume. The reduced actor then received Win32 access-denied error 5 while opening a writable directory in the additional nested fixture, before filler allocation or disk-full controls. The exact access-check cause is unresolved; this is no evidence of a staging-write failure or retry. The earlier transaction and process-interruption cases still passed. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46033460924/job/140622829361)

The next fixture uses the empty owned directory whose protected TokenUser ACL was already applied and read back, matching the process-interruption fixture setup. It retains the original directory scope and checks its complete identity against the production writable scope under the same reduced actor. Progress markers distinguish initial scope opening, baseline creation and filler allocation. This changes no production permissions, access mask, token or transaction behavior. Native qualification remains pending.

Run `46035306785` at `373c332` again provisioned the bounded volume, then the first writable scope open returned access-denied error 5, even on that verified directory. Baseline creation and filling never ran. The access-check cause is still unknown; changing the fixture directory did not resolve it. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46035306785/job/140629088949)

After this specific failure, a test-only diagnostic now opens the existing leaf under its held verified ancestor. It compares the pinned capability library's exact native request with one varying only the security QoS pointer, then probes each additional access right independently. Successful handles must report the same directory identity and are closed without writing. A READ_CONTROL handle reports redacted DACL/owner/group comparisons and the mandatory integrity label; the effective token's integrity RID and mandatory policy are recorded without account SIDs or raw handles. All probes use FILE_OPEN, and the original error is returned regardless of diagnostic outcomes. Local Windows-target strict Clippy, formatting, the 283-file source gate and documentation validation passed before native diagnostic execution. [Access probes](../src/windows/create/stage/enospc/access.rs) · [Security observations](../src/windows/stage_security/diagnostics.rs)

Run `46037995281` at `da0f14d` reproduced the denial with a high-integrity reduced actor and an unlabeled directory whose protected ACL contained the single full-control user ACE. The exact native request and the request with a null QoS pointer both returned STATUS_ACCESS_DENIED. The read base and READ_CONTROL succeeded. Each request adding only one write right failed, covering file creation, subdirectory creation, extended attributes and attributes. The disk-full fixture stopped before baseline creation and filling. [Windows diagnostics](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46037995281/job/140638210134)

The next fixture gives the existing runner user the only full-control ACE on the new VHD backing file at creation. It reads back that protected DACL before attaching the exact owned image in ReadWrite mode. The reduced actor verifies full control against the captured backing descriptor with AccessCheck. The test also checks the held NTFS volume's read-only flag. On another writable-open denial, a diagnostic request adds explicit directory intent and retains the existing no-follow and no-recall options. This configuration tests the VHD attachment user's access as a possible additional gate. The Storage provider does not document which descriptor it supplies when attaching the image. Native results remain pending and public writes remain disabled. [VHD attachment security](https://learn.microsoft.com/en-us/windows/win32/api/virtdisk/nf-virtdisk-attachvirtualdisk) · [Storage provider mount](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/msft-diskimage-mount)

### Qualified transaction and create-only activation

Run `46044058571` at `54348aa` passed all 11 jobs. The owned backing image retained the protected single-user DACL, the mounted NTFS volume reported writable, and the reduced actor's AccessCheck granted `0x001f01ff`. It opened both writable scopes and created the baseline. The independent control returned Win32 `ERROR_DISK_FULL` (112), retained its 4,096-byte synced prefix and passed exact prefix read-back. The actual candidate returned the same native error from `write pending file` after only the Created checkpoint. It rejected unchanged, preserved the original bytes and fingerprint, and left no destination or pending stage. Removing only the fixture filler reclaimed 109,940,736 bytes, after which the explicit retry returned a verified receipt matching exact read-back. VHD detachment and cleanup passed. The normal Windows tests, legacy reduced-token probe, 20 transaction scenarios, three process cuts and five single-instance probes also passed. [Windows result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46044058571/job/140658767295)

This qualifies that fixture configuration; it does not establish which descriptor the Storage provider used in the earlier denials. The writer still checks each actual destination and refuses unqualified scopes before staging.

Public Windows `commit(CreateNew)` now delegates to this same transaction. Explicit desktop Save As, Export Copy and external signing use the independent create-copy capability. CLI output copies and adjacent snapshots/JSON sidecars use the same generic create path. Existing-file replacement, managed desktop backups/restore and implicit close/navigation saves remain unavailable on Windows. Destinations directly in a drive root are refused before staging; the historical ReOpenFile branch was not qualified and is excluded from activation. No transaction behavior is broadened for replacement.

Local strict Clippy with the actual desktop feature enabled passed for Windows x64 and macOS arm64. All 112 local native desktop tests and seven CLI write cases passed on macOS. The source and documentation gates passed. Newly enabled native Windows CLI and desktop callback cases compile but still require hosted execution. They cover source preservation, occupied destinations, dirty export state, verified Save As acknowledgement and external signing. No native picker or game process is launched.

Run `46058400674` at `0ab8ef3` passed all 257 Windows native tests, including all seven actual CLI write cases, exact adjacent snapshot/sidecar verification and unchanged unsupported restore. The Windows desktop job passed all 100 tests and packaged navigation. Its named Save As case rejected stale/colliding requests without acknowledging or changing the original, then adopted only the verified new target. Export retained the dirty document and original selection. External signing created a valid signed copy, preserved its source and existing collision target, and consumed its source authority at the appropriate outcome. The NTFS disk-full and process cases passed again. These are native callbacks with actual filesystem effects; interactive dialogs and Windows WebView behavior were not exercised. [Windows native](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46058400674/job/140706422755) · [Windows desktop](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46058400674/job/140706422824)

Ten of the run's eleven jobs passed. The macOS native job passed both revised exhaustion cases but failed ordinary image teardown after an initial busy detach and five unresolved mount/image transition checks. The log did not identify which transition persisted. That separate fixture cleanup failure does not make this an all-green workspace result. [macOS result](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46058400674/job/140706422665)
