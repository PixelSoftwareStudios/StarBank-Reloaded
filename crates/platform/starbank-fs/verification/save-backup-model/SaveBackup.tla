----------------------------- MODULE SaveBackup -----------------------------
CONSTANT Fault
VARIABLE s

Modes == {"SaveProtected", "SaveWithoutBackup", "Restore"}
LiveVersions == {"Original", "Candidate", "External"}
Phases == {"Preparing", "Check", "BackupBytes", "BackupReady", "Staged",
           "Published", "DirectorySynced", "StorageVerified", "Done"}
Outcomes == {"Pending", "PolicyRejected", "BackupRejected", "BackupUncertain",
             "MetadataRejected", "MetadataUncertain", "RejectedUnchanged",
             "CommitUncertain", "CommittedVerified", "ProcessCrashed"}
Faults == {"None", "SkipBackup", "IgnoreDisplacedConflict", "BlindRetry"}

ASSUME Fault \in Faults

Init ==
    \E mode \in Modes:
        s = [mode |-> mode, phase |-> "Preparing", live |-> "Original",
             backup |-> "None", captured |-> "Unknown",
             restoreChecked |-> FALSE, sibling |-> "Absent",
             race |-> "None", exchanged |-> FALSE, synced |-> FALSE,
             observed |-> "Unknown", recoveryNamed |-> FALSE,
             outcome |-> "Pending", editor |-> "Unchanged", trusted |-> TRUE,
             crashAt |-> "None", reopened |-> "Unknown", retry |-> "None"]

NeedsBackup == s.mode # "SaveWithoutBackup"

Approve ==
    /\ s.phase = "Preparing"
    /\ s' = [s EXCEPT !.phase = "Check",
                      !.restoreChecked = (s.mode = "Restore")]

CaptureBackupBytes ==
    /\ s.phase = "Check"
    /\ NeedsBackup
    /\ s.live = "Original"
    /\ s' = [s EXCEPT !.phase = "BackupBytes", !.backup = "BytesVerified",
                      !.captured = s.live]

RecordBackup ==
    /\ s.phase = "BackupBytes"
    /\ s' = [s EXCEPT !.phase = "BackupReady", !.backup = "Recorded"]

BackupFailure ==
    \/ /\ s.phase = "Check"
       /\ NeedsBackup
       /\ s.live = "Original"
       /\ \E failure \in {"BackupRejected", "BackupUncertain"}:
              s' = [s EXCEPT !.phase = "Done", !.outcome = failure]
    \/ /\ s.phase = "BackupBytes"
       /\ \E failure \in {"MetadataRejected", "MetadataUncertain"}:
              s' = [s EXCEPT !.phase = "Done", !.outcome = failure]

PolicyReject ==
    \/ /\ s.phase = "Preparing"
       /\ s' = [s EXCEPT !.phase = "Done", !.outcome = "PolicyRejected"]
    \/ /\ s.phase = "Check"
       /\ NeedsBackup
       /\ s.live # "Original"
       /\ s' = [s EXCEPT !.phase = "Done", !.outcome = "PolicyRejected"]

Stage ==
    /\ s.phase \in {"Check", "BackupReady"}
    /\ ~NeedsBackup \/ s.backup = "Recorded" \/ Fault = "SkipBackup"
    /\ s.live = "Original"
    /\ s' = [s EXCEPT !.phase = "Staged", !.sibling = "Candidate"]

KnownNativeReject ==
    /\ s.phase \in {"Check", "BackupReady", "Staged"}
    /\ (s.phase # "Check" \/ ~NeedsBackup)
    /\ \E sibling \in IF s.phase = "Staged" THEN {"Absent", "Candidate"} ELSE {"Absent"}:
           s' = [s EXCEPT !.phase = "Done", !.outcome = "RejectedUnchanged",
                         !.sibling = sibling]

Exchange ==
    /\ s.phase = "Staged"
    /\ s' = [s EXCEPT !.phase = "Published", !.live = "Candidate",
                      !.sibling = s.live, !.exchanged = TRUE]

(* A generic rename error need not tell the caller whether exchange took effect. *)
AmbiguousExchange ==
    /\ s.phase = "Staged"
    /\ \E changed \in BOOLEAN:
           s' = [s EXCEPT !.phase = "Done", !.outcome = "CommitUncertain",
                         !.trusted = FALSE, !.recoveryNamed = TRUE,
                         !.live = IF changed THEN "Candidate" ELSE s.live,
                         !.sibling = IF changed THEN s.live ELSE s.sibling,
                         !.exchanged = changed]

DirectorySync ==
    /\ s.phase = "Published"
    /\ s' = [s EXCEPT !.phase = "DirectorySynced", !.synced = TRUE]

PostPublicationFailure ==
    /\ s.phase \in {"Published", "DirectorySynced"}
    /\ s' = [s EXCEPT !.phase = "Done", !.outcome = "CommitUncertain",
                      !.trusted = FALSE, !.recoveryNamed = TRUE]

VerifyStorage ==
    /\ s.phase = "DirectorySynced"
    /\ s.live = "Candidate"
    /\ s.sibling = "Original" \/ Fault = "IgnoreDisplacedConflict"
    /\ s' = [s EXCEPT !.phase = "StorageVerified", !.observed = s.live,
                      !.recoveryNamed = TRUE]

HostFinish ==
    /\ s.phase = "StorageVerified"
    /\ \E acknowledged \in BOOLEAN:
           s' = [s EXCEPT !.phase = "Done", !.outcome = "CommittedVerified",
                         !.editor = IF acknowledged THEN "Acknowledged" ELSE "NeedsReopen",
                         !.trusted = acknowledged]

Crash ==
    /\ s.phase # "Done"
    /\ s' = [s EXCEPT !.phase = "Done", !.outcome = "ProcessCrashed",
                      !.crashAt = s.phase, !.trusted = FALSE, !.editor = "Lost"]

ExternalRace ==
    /\ s.phase # "Done"
    /\ s.race = "None"
    /\ s' = [s EXCEPT !.live = "External",
                      !.race = CASE s.phase = "Staged" -> "AfterCheck"
                                 [] s.phase \in {"Published", "DirectorySynced"} -> "AfterExchange"
                                 [] s.phase = "StorageVerified" -> "AfterVerify"
                                 [] OTHER -> "BeforeCheck"]

RetryBeforeReopen ==
    /\ s.outcome = "CommitUncertain"
    /\ ~s.trusted
    /\ s.retry = "None"
    /\ s' = [s EXCEPT !.retry = IF Fault = "BlindRetry" THEN "Admitted" ELSE "Blocked"]

Reopen ==
    /\ s.phase = "Done"
    /\ ~s.trusted
    /\ s' = [s EXCEPT !.trusted = TRUE, !.reopened = s.live]

Advance ==
    \/ Approve \/ CaptureBackupBytes \/ RecordBackup \/ BackupFailure
    \/ PolicyReject \/ Stage \/ KnownNativeReject \/ Exchange \/ AmbiguousExchange
    \/ DirectorySync \/ PostPublicationFailure \/ VerifyStorage \/ HostFinish \/ Crash

Next ==
    \/ Advance \/ ExternalRace \/ RetryBeforeReopen \/ Reopen
    \/ /\ s.phase = "Done"
       /\ UNCHANGED s

Spec == Init /\ [][Next]_s /\ WF_s(Advance)

TypeOK ==
    /\ s.mode \in Modes
    /\ s.phase \in Phases
    /\ s.live \in LiveVersions
    /\ s.backup \in {"None", "BytesVerified", "Recorded"}
    /\ s.captured \in LiveVersions \cup {"Unknown"}
    /\ s.restoreChecked \in BOOLEAN
    /\ s.sibling \in LiveVersions \cup {"Absent"}
    /\ s.race \in {"None", "BeforeCheck", "AfterCheck", "AfterExchange", "AfterVerify"}
    /\ s.exchanged \in BOOLEAN
    /\ s.synced \in BOOLEAN
    /\ s.observed \in LiveVersions \cup {"Unknown"}
    /\ s.recoveryNamed \in BOOLEAN
    /\ s.outcome \in Outcomes
    /\ s.editor \in {"Unchanged", "Acknowledged", "NeedsReopen", "Lost"}
    /\ s.trusted \in BOOLEAN
    /\ s.crashAt \in Phases \cup {"None"}
    /\ s.reopened \in LiveVersions \cup {"Unknown"}
    /\ s.retry \in {"None", "Blocked", "Admitted"}
    /\ (s.phase = "Done") = (s.outcome # "Pending")

ProtectedExchangeHasRecordedBackup ==
    (s.exchanged /\ NeedsBackup) =>
        /\ s.backup = "Recorded"
        /\ s.captured = "Original"

RestoreExchangeUsesCheckedSelection ==
    (s.exchanged /\ s.mode = "Restore") => s.restoreChecked

KnownRejectionDidNotExchange ==
    s.outcome \in {"PolicyRejected", "BackupRejected", "BackupUncertain",
                  "MetadataRejected", "MetadataUncertain", "RejectedUnchanged"}
        => /\ ~s.exchanged
           /\ s.editor = "Unchanged"

BackupFailurePreservesVerifiedData ==
    s.outcome \in {"MetadataRejected", "MetadataUncertain"} =>
        /\ s.backup = "BytesVerified"
        /\ s.captured = "Original"

DisplacedVersionRetained ==
    s.exchanged => s.sibling = IF s.race = "AfterCheck" THEN "External" ELSE "Original"

VerifiedAfterSyncAndBothObservations ==
    (s.phase = "StorageVerified" \/ s.outcome = "CommittedVerified")
        => /\ s.exchanged
           /\ s.synced
           /\ s.observed = "Candidate"
           /\ s.sibling = "Original"
           /\ s.recoveryNamed

OnlyVerifiedAcknowledged ==
    s.editor = "Acknowledged" => s.outcome = "CommittedVerified"

UncertainRequiresReopen ==
    (s.outcome = "CommitUncertain")
        => /\ s.recoveryNamed
           /\ s.editor = "Unchanged"
           /\ s.trusted = (s.reopened # "Unknown")

RetryRequiresReopen ==
    s.retry # "Admitted"

CrashHasNoAcknowledgement ==
    s.outcome = "ProcessCrashed"
        => /\ s.editor = "Lost"
           /\ s.crashAt # "None"
           /\ s.trusted = (s.reopened # "Unknown")

CrashArtifactsMatchCut ==
    s.outcome = "ProcessCrashed" =>
        CASE s.crashAt = "Staged" -> ~s.exchanged /\ s.sibling = "Candidate"
          [] s.crashAt \in {"Published", "DirectorySynced", "StorageVerified"} ->
                 s.exchanged /\ s.sibling \in {"Original", "External"}
          [] OTHER -> ~s.exchanged /\ s.sibling = "Absent"

LiveChangesOnlyOnExchangeOrExternalRace ==
    [][s.live = s'.live \/ (~s.exchanged /\ s'.exchanged) \/ s.race # s'.race]_s

EventuallyTerminates ==
    <> (s.phase = "Done")
=============================================================================
