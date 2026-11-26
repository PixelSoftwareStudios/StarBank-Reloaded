# Native save and backup model

This TLA+ model checks part of VO-05 under EVS-1. It covers one active-bank overwrite or restore into an existing authorized macOS target. The three initial cases are a save that requires a backup, an approved save that requires none, and a restore that requires a reversal backup. The initial backup policy is already resolved for this attempt. The model uses a single cooperative operation, one optional competing write, and one optional process termination. Same bytes under a different file identity count as a different version.

## Run

Use the TLA+ Tools v1.7.4 release jar and Java 21. The script checks the jar SHA-256, uses TLC 2.19 with one worker, and writes the checked configurations, state files and logs to a fresh temporary directory. TLC 2.19 creates an ephemeral Java RMI listener for its local fingerprint table. The normal model and three counterexample checks require no external service.

```sh
sh crates/platform/starbank-fs/verification/save-backup-model/run.sh /path/to/tla2tools.jar
```

The normal model must exit successfully. Each negative control must reach the specific invariant violation expected by the runner. The fault switches apply only to the model.

## Source boundaries

| Model transition | Implementation |
| --- | --- |
| `Approve` and `PolicyReject` | The desktop revalidates the active document, revision, path and consent. Restore prepares the selected scoped snapshot and rechecks the selected bytes against catalog integrity immediately before capturing a reversal. The model groups policy refusal before a live write and abstracts parsing, confirmation and identity checks as observations. [Save host](../../../../../apps/starbank-desktop/src-tauri/src/save.rs#L229) · [Restore host](../../../../../apps/starbank-desktop/src-tauri/src/backup.rs#L199) · [Native restore](../../src/backup_catalog.rs#L288) |
| `CaptureBackupBytes`, `RecordBackup` and `BackupFailure` | Protected overwrite and restore capture the current expected bytes. After exclusive backup creation and read-back, the sidecar must also publish with a verified receipt before live storage is called. Uncertain backup creation and rejected or uncertain metadata stop the attempt. The two success steps are observations of the original captured bytes and the associated metadata. Naming collisions and attempts to write individual backup files are abstracted into their eventual outcomes. [Backup policy](../../../../domain/starbank-backup/src/operations.rs#L102) · [Exclusive sidecar](../../src/backup_storage.rs#L60) |
| `Stage` and `KnownNativeReject` | Native storage stages the new bytes in a sibling under the directory lease, checks the expected fingerprint and staged metadata, and then reaches the Staged checkpoint. A known pre-publication failure or a recognized no-effect publication failure returns `RejectedUnchanged`. Cleanup can leave a candidate sibling if removing it fails. A competing writer detected here can already have changed the live path. [Native writer](../../src/native/commit.rs#L72) · [Stage cleanup](../../src/native/stage.rs#L74) |
| `Exchange`, `AmbiguousExchange` and `ExternalRace` | After the last pre-publication check, APFS exchange can still displace a competing version. Unknown native rename results return `CommitUncertain`; the model allows exchange to have happened or stayed unapplied. A competing write may also occur after exchange, or after successful read-back and before editor acknowledgement. The modeled sibling holds what was displaced, while the adapter's uncertain result supplies a recovery location for inspection. [Publication](../../src/native/commit.rs#L125) · [Late writer tests](../../src/native/commit/tests.rs#L160) |
| `DirectorySync`, `VerifyStorage` and `PostPublicationFailure` | A verified receipt follows the directory sync, read-back of the candidate and a check of the displaced expected file, including security metadata. Publication followed by a failed sync or observation is uncertain. Verified storage describes what the writer observed, since external writes can still occur later. [Sync and receipt](../../src/native/commit.rs#L144) · [Displaced-file verification](../../src/native/commit.rs#L249) |
| `HostFinish`, `RetryBeforeReopen` and `Reopen` | Only verified storage permits the desktop to acknowledge the save or activate the restored editor. An uncertain active write invalidates its expected fingerprint. A new authorized open can supply a fresh fingerprint. The model ends the old attempt and checks retry admission before a fresh open. An acknowledgement failure also leaves the verified write requiring reopen. [Save acknowledgement and uncertainty](../../../../../apps/starbank-desktop/src-tauri/src/save.rs#L358) · [Restore acknowledgement](../../../../../apps/starbank-desktop/src-tauri/src/backup.rs#L313) |
| `Crash` | Process death stops the attempt without editor acknowledgement. For the Staged, Published and DirectorySynced cuts, the concrete fixture verifies the live and sibling bytes and lease release after SIGKILL. The earlier backup cuts and the verified-before-acknowledgement cut are model transitions. [Process-kill test](../../src/native/commit/tests.rs#L216) |

The model checks that protected exchange requires verified backup bytes and a recorded sidecar, restored bytes come from a checked selection, known rejection performs no exchange, successful acknowledgement follows verified storage, and uncertainty prevents blind retry. It also checks retained displaced versions, modeled crash cuts, and that only exchange or an external write changes the live version. With weak fairness on the union of progress steps, the attempt eventually reaches a terminal state. A terminal state may be failure or process death. An explicit reopen after the terminal state is outside this fairness assumption.

Symbolic versions represent identity, bytes and required security observations. The model assumes native operations implement the modeled exchange and observation contracts and limits competing changes to the live path. Sidecar or staging-path tampering, multiple competing writers, repeated transactions, Save As, Windows/Linux replacement, filesystem failure details, physical power loss and refinement of Rust or kernel implementations remain outside scope. Model checks and native fault-injection tests provide separate evidence.

## Observed result

At `e2cf03533264146e23815f38f1353c2d5c829555` plus this verification directory, TLC 2.19 (revision `5a47802`) on macOS 14.7.2 arm64 and Temurin 21.0.8 completed with 568 generated states, 298 distinct states, depth 11 and an empty queue. The temporal checks also completed. The normal configuration exited zero. Each negative control exited 12 with its expected violation:

| Model-only fault | Invariant violated |
| --- | --- |
| `SkipBackup` | `ProtectedExchangeHasRecordedBackup` |
| `IgnoreDisplacedConflict` | `VerifiedAfterSyncAndBothObservations` |
| `BlindRetry` | `RetryRequiresReopen` |

On the same clean source revision, the following Rust 1.100.0 command passed 32 tests. The process-kill worker is ignored during discovery and invoked by its parent test.

```sh
cargo +1.100.0 test --offline --locked -p starbank-fs -p starbank-backup
```
