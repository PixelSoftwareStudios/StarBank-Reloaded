use std::{io, num::NonZeroUsize};

use sc2bank::Limits;
use starbank_backup::{BackupError, RecordFailure};
use starbank_fs::{CommitIssue, NativeBackupCatalog, NativeBackupError, StorageError};

use crate::failure::{ErrorCode, Failure, Status};

pub(crate) fn catalog(limits: Limits) -> Result<NativeBackupCatalog, Failure> {
	NonZeroUsize::new(limits.document_bytes)
		.map(NativeBackupCatalog::new)
		.ok_or_else(|| {
			Failure::new(
				Status::Validation,
				ErrorCode::InputLimit,
				"The backup limit is zero.",
			)
		})
}

impl From<NativeBackupError> for Failure {
	fn from(error: NativeBackupError) -> Self {
		match error {
			NativeBackupError::Storage(error) => error.into(),
			NativeBackupError::Policy(error) => (*error).into(),
			NativeBackupError::InvalidSource => Self::new(
				Status::Validation,
				ErrorCode::InvalidBackup,
				"Select a bank with a portable .SC2Bank filename and no parent traversal for adjacent backups.",
			),
			NativeBackupError::Timestamp => Self::new(
				Status::Io,
				ErrorCode::Io,
				"The current UTC time is invalid.",
			),
			NativeBackupError::DirectoryLimit => Self::new(
				Status::Validation,
				ErrorCode::InputLimit,
				"The backup directory contains too many entries.",
			),
			_ => Self::new(
				Status::Validation,
				ErrorCode::InvalidBackup,
				"The backup metadata does not identify this bank and snapshot.",
			),
		}
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
