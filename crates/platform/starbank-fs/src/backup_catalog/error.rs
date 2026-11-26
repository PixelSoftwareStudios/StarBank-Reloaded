use std::{error::Error, fmt};

use starbank_backup::BackupError;

use crate::{CommitIssue, StorageError};

type PolicyError = BackupError<StorageError, CommitIssue>;

/// Failure before attempting to write live bank bytes.
#[derive(Debug)]
#[non_exhaustive]
pub enum NativeBackupError {
	/// The selected bank path cannot be used as a portable backup identity.
	InvalidSource,
	/// The backup location must be absolute and contain no parent traversal.
	InvalidLocation,
	/// The machine's current UTC time cannot be formatted for a backup name.
	Timestamp,
	/// The selected backup directory exceeded its scan limit.
	DirectoryLimit,
	/// Sidecar, selected bank and snapshot failed consistency checks.
	MetadataMismatch,
	/// A native storage operation failed.
	Storage(StorageError),
	/// A backup policy precondition failed with any available recovery evidence.
	Policy(Box<PolicyError>),
}

impl fmt::Display for NativeBackupError {
	fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidSource => {
				out.write_str("backup source requires a portable .sc2bank filename")
			}
			Self::InvalidLocation => {
				out.write_str("backup location requires an absolute path without parent traversal")
			}
			Self::Timestamp => {
				out.write_str("current UTC time cannot be used for a backup timestamp")
			}
			Self::DirectoryLimit => out.write_str("backup directory contains too many entries"),
			Self::MetadataMismatch => {
				out.write_str("backup metadata does not identify this bank and snapshot")
			}
			Self::Storage(error) => fmt::Display::fmt(error, out),
			Self::Policy(error) => fmt::Display::fmt(error, out),
		}
	}
}

impl Error for NativeBackupError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Storage(error) => Some(error),
			Self::Policy(error) => Some(error.as_ref()),
			_ => None,
		}
	}
}

impl From<StorageError> for NativeBackupError {
	fn from(error: StorageError) -> Self {
		Self::Storage(error)
	}
}

impl From<PolicyError> for NativeBackupError {
	fn from(error: PolicyError) -> Self {
		Self::Policy(Box::new(error))
	}
}
