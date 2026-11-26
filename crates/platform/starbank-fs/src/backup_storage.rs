use std::{io, path::Path};

use starbank_backup::{
	BackupRecord, BackupStorage, CommitState, ReadFailure, RecordFailure, Snapshot, StorageIntent,
};

use crate::{
	CommitIntent, CommitIssue, CommitOutcome, CommitReceipt, Fingerprint, StorageError,
	backup_metadata::{MAX_METADATA_BYTES, Metadata, sidecar_path},
	commit, read_snapshot,
};

pub(super) struct NativeStorage;

impl BackupStorage for NativeStorage {
	type Fingerprint = Fingerprint;
	type Receipt = CommitReceipt;
	type Error = StorageError;
	type Issue = CommitIssue;

	fn read(
		&mut self,
		path: &Path,
		max_bytes: usize,
	) -> Result<Snapshot<Self::Fingerprint>, ReadFailure<Self::Error>> {
		match read_snapshot(path, max_bytes) {
			Ok(snapshot) => Ok(Snapshot {
				fingerprint: snapshot.fingerprint().clone(),
				bytes: snapshot.into_bytes(),
			}),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Err(ReadFailure::Missing),
			Err(error) => Err(ReadFailure::Failed(error)),
		}
	}

	fn commit(
		&mut self,
		path: &Path,
		bytes: &[u8],
		intent: StorageIntent<'_, Self::Fingerprint>,
		max_bytes: usize,
	) -> CommitState<Self::Receipt, Self::Error, Self::Issue> {
		let intent = match intent {
			StorageIntent::CreateNew => CommitIntent::CreateNew,
			StorageIntent::ReplaceExpected(expected) => CommitIntent::ReplaceExpected(expected),
		};
		match commit(path, bytes, intent, max_bytes) {
			CommitOutcome::CommittedVerified(receipt) => CommitState::Verified(receipt),
			CommitOutcome::RejectedUnchanged(error)
				if matches!(intent, CommitIntent::CreateNew)
					&& error.kind() == io::ErrorKind::AlreadyExists =>
			{
				CommitState::AlreadyExists
			}
			CommitOutcome::RejectedUnchanged(error) => CommitState::Rejected(error),
			CommitOutcome::CommitUncertain(issue) => CommitState::Uncertain(issue),
		}
	}

	fn record_backup(
		&mut self,
		record: &BackupRecord,
	) -> Result<(), RecordFailure<Self::Error, Self::Issue>> {
		let metadata = Metadata::from_record(record).map_err(RecordFailure::Rejected)?;
		let bytes = serde_json::to_vec(&metadata).map_err(|error| {
			RecordFailure::Rejected(StorageError::Io {
				operation: "serialize backup record",
				source: error.into(),
			})
		})?;
		let path = sidecar_path(record.id().path());
		match commit(&path, &bytes, CommitIntent::CreateNew, MAX_METADATA_BYTES) {
			CommitOutcome::CommittedVerified(_) => Ok(()),
			CommitOutcome::RejectedUnchanged(error) => Err(RecordFailure::Rejected(error)),
			CommitOutcome::CommitUncertain(issue) => Err(RecordFailure::Uncertain(issue)),
		}
	}
}
