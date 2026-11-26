use std::{io, path::Path};

use starbank_backup::{
	BackupRecord, BackupStorage, CommitState, ReadFailure, RecordFailure, Snapshot, StorageIntent,
};

use crate::{
	BackupDirectory, CommitIntent, CommitIssue, CommitOutcome, CommitReceipt, FileSnapshot,
	Fingerprint, StorageError,
	backup_metadata::{MAX_METADATA_BYTES, Metadata, sidecar_path},
	commit, read_snapshot,
};

#[derive(Clone, Copy)]
pub(super) struct NativeStorage<'a> {
	destination: Option<&'a BackupDirectory>,
	selected: Option<&'a BackupDirectory>,
	read_limit: usize,
}

impl<'a> NativeStorage<'a> {
	pub(super) fn new(destination: Option<&'a BackupDirectory>) -> Self {
		Self {
			destination,
			selected: None,
			read_limit: usize::MAX,
		}
	}

	pub(super) fn with_selected(mut self, source: Option<&'a BackupDirectory>) -> Self {
		self.selected = source;
		self
	}

	pub(super) fn with_read_limit(mut self, limit: usize) -> Self {
		self.read_limit = limit;
		self
	}

	pub(super) fn read_file(
		&self,
		path: &Path,
		max_bytes: usize,
	) -> Result<FileSnapshot, StorageError> {
		match [self.selected, self.destination]
			.into_iter()
			.flatten()
			.find(|directory| path.parent() == Some(directory.path()))
		{
			Some(directory) => directory.read(path, max_bytes),
			None => read_snapshot(path, max_bytes),
		}
	}

	fn write_file(
		&self,
		path: &Path,
		bytes: &[u8],
		intent: CommitIntent<'_>,
		max_bytes: usize,
	) -> CommitOutcome {
		match self
			.destination
			.filter(|directory| path.parent() == Some(directory.path()))
		{
			Some(directory) => directory.commit(path, bytes, intent, max_bytes),
			None => commit(path, bytes, intent, max_bytes),
		}
	}
}

impl BackupStorage for NativeStorage<'_> {
	type Fingerprint = Fingerprint;
	type Receipt = CommitReceipt;
	type Error = StorageError;
	type Issue = CommitIssue;

	fn read(
		&mut self,
		path: &Path,
		max_bytes: usize,
	) -> Result<Snapshot<Self::Fingerprint>, ReadFailure<Self::Error>> {
		match self.read_file(path, max_bytes.min(self.read_limit)) {
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
		match self.write_file(path, bytes, intent, max_bytes) {
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
		match self.write_file(&path, &bytes, CommitIntent::CreateNew, MAX_METADATA_BYTES) {
			CommitOutcome::CommittedVerified(_) => Ok(()),
			CommitOutcome::RejectedUnchanged(error) => Err(RecordFailure::Rejected(error)),
			CommitOutcome::CommitUncertain(issue) => Err(RecordFailure::Uncertain(issue)),
		}
	}
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
