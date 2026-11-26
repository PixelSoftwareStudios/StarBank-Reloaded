use std::{
	io,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use sc2bank::Limits;
use starbank_backup::{
	BackupCatalog, BackupError, BackupRequest, BackupStorage, BackupTimestamp, BankSource,
	CommitState, Overwrite, OverwriteBackup, ReadFailure, RecordFailure, Snapshot, StorageIntent,
};
use starbank_fs::{
	CommitIntent, CommitIssue, CommitOutcome, CommitReceipt, Fingerprint, StorageError, commit,
	read_snapshot,
};
use time::OffsetDateTime;

use crate::{
	backup_metadata::{MAX_METADATA_BYTES, Metadata, sidecar_path},
	failure::{ErrorCode, Failure, Status},
	write_bank::BackupPreference,
};

pub(crate) struct NativeStorage;

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
		record: &starbank_backup::BackupRecord,
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

pub(crate) struct BackupEffect {
	pub(crate) backup: Option<PathBuf>,
	pub(crate) commit: CommitOutcome,
}

pub(crate) fn overwrite(
	path: &Path,
	expected: &Fingerprint,
	bytes: &[u8],
	preference: BackupPreference,
	limits: Limits,
) -> Result<BackupEffect, Failure> {
	allow_live_replacement()?;
	let source = bank_source(path)?;
	let timestamp = timestamp()?;
	let policy = match preference {
		BackupPreference::Automatic => {
			OverwriteBackup::FirstInSession(BackupRequest::adjacent(timestamp))
		}
		BackupPreference::Requested => OverwriteBackup::Explicit(timestamp),
	};
	let effect = catalog(limits)?.overwrite(
		&mut NativeStorage,
		&source,
		Overwrite {
			expected,
			bytes,
			backup: policy,
		},
	)?;
	Ok(BackupEffect {
		backup: effect.backup.map(|id| id.path().to_owned()),
		commit: commit_outcome(effect.commit),
	})
}

pub(crate) fn allow_live_replacement() -> Result<(), Failure> {
	if cfg!(target_os = "macos") {
		Ok(())
	} else {
		Err(StorageError::Unsupported("verified native replacement requires local APFS").into())
	}
}

pub(crate) fn bank_source(path: &Path) -> Result<BankSource, Failure> {
	if path
		.components()
		.any(|component| component == std::path::Component::ParentDir)
	{
		return Err(invalid_bank_source());
	}
	BankSource::new(std::path::absolute(path)?).map_err(|_| invalid_bank_source())
}

fn invalid_bank_source() -> Failure {
	Failure::new(
		Status::Validation,
		ErrorCode::InvalidBackup,
		"Select a bank with a portable .SC2Bank filename and no parent traversal for adjacent backups.",
	)
}

pub(crate) fn catalog(limits: Limits) -> Result<BackupCatalog, Failure> {
	NonZeroUsize::new(limits.document_bytes)
		.map(BackupCatalog::new)
		.ok_or_else(|| {
			Failure::new(
				Status::Validation,
				ErrorCode::InputLimit,
				"The backup limit is zero.",
			)
		})
}

pub(crate) fn timestamp() -> Result<BackupTimestamp, Failure> {
	let now = OffsetDateTime::now_utc();
	let (year, month, day) = now.to_calendar_date();
	let (hour, minute, second) = now.to_hms();
	format!(
		"{year:04}{month:02}{day:02}_{hour:02}{minute:02}{second:02}",
		month = u8::from(month)
	)
	.parse()
	.map_err(|_| {
		Failure::new(
			Status::Io,
			ErrorCode::Io,
			"The current UTC time is invalid.",
		)
	})
}

pub(crate) fn commit_outcome(
	effect: CommitState<CommitReceipt, StorageError, CommitIssue>,
) -> CommitOutcome {
	match effect {
		CommitState::Verified(receipt) => CommitOutcome::CommittedVerified(receipt),
		CommitState::Rejected(error) => CommitOutcome::RejectedUnchanged(error),
		CommitState::Uncertain(issue) => CommitOutcome::CommitUncertain(issue),
		CommitState::AlreadyExists => CommitOutcome::RejectedUnchanged(StorageError::Conflict),
	}
}

impl From<BackupError<StorageError, CommitIssue>> for Failure {
	fn from(error: BackupError<StorageError, CommitIssue>) -> Self {
		match error {
			BackupError::Storage(error) => error.into(),
			BackupError::TargetChanged => StorageError::Conflict.into(),
			BackupError::UncertainBackup { path, issue } => Failure::new(
				Status::Uncertain,
				ErrorCode::BackupUncertain,
				"Backup publication needs inspection. The live bank was left unchanged.",
			)
			.with_backup_path(&path)
			.with_recovery_path(issue.recovery().path()),
			BackupError::Metadata { record, failure } => {
				let mut failure: Self = match failure {
					RecordFailure::Rejected(_) => Failure::new(
						Status::Io,
						ErrorCode::BackupMetadata,
						"The snapshot exists, but its metadata could not be published.",
					),
					RecordFailure::Uncertain(issue) => Failure::new(
						Status::Uncertain,
						ErrorCode::BackupUncertain,
						"Backup metadata publication needs inspection. The live bank was left unchanged.",
					)
					.with_recovery_path(issue.recovery().path()),
				};
				failure = failure.with_backup_path(record.id().path());
				failure
			}
			BackupError::MissingFile => io::Error::from(io::ErrorKind::NotFound).into(),
			_ => Failure::new(
				Status::Validation,
				ErrorCode::InvalidBackup,
				"The backup could not be found, verified or created for the selected bank.",
			),
		}
	}
}
