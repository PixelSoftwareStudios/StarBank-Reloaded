use std::{fmt::Display, path::Path};

use serde::Serialize;
use starbank_backup::{BackupError, BackupId, RecordFailure};
use starbank_fs::NativeBackupError;

use crate::{commands::CommandError, save::DesktopSession};

const MAX_DISPLAY_PATH_BYTES: usize = 4_096;
const MAX_REASON_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryOperation {
	SaveOverwrite,
	SaveAs,
	Restore,
	ExportCopy,
	BackupCopy,
	ExternalSignedCopy,
}

impl RecoveryOperation {
	pub(crate) fn affects_active(self) -> bool {
		matches!(self, Self::SaveOverwrite | Self::Restore)
	}
}

pub(crate) struct WriteContext<'path> {
	pub(crate) operation: RecoveryOperation,
	pub(crate) destination: &'path Path,
	pub(crate) backup: Option<&'path BackupId>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryOutcome {
	CommitUncertain,
	LiveUnchanged,
}

/// Display evidence from one attempted operation, never authority to open or overwrite a path.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryReceipt {
	operation: RecoveryOperation,
	outcome: RecoveryOutcome,
	affects_active: bool,
	destination: DisplayPath,
	recovery: RecoveryLocation,
	backup: BackupEvidence,
	reason: String,
	reason_truncated: bool,
}

#[derive(Debug, Serialize)]
struct DisplayPath {
	display: String,
	truncated: bool,
	lossy: bool,
}

impl DisplayPath {
	fn new(path: &Path) -> Self {
		let (display, truncated) = bounded(&path.to_string_lossy(), MAX_DISPLAY_PATH_BYTES);
		Self {
			display,
			truncated,
			lossy: path.to_str().is_none(),
		}
	}
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RecoveryLocation {
	/// Location recorded at the attempt; the artifact may have moved or changed since then.
	Recorded {
		path: DisplayPath,
	},
	Unknown,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BackupEvidence {
	/// Snapshot and metadata were verified before attempting the live write.
	Verified {
		path: DisplayPath,
	},
	/// Publication or metadata is incomplete; this is a recorded location, not a usable backup.
	Unverified {
		path: DisplayPath,
	},
	NotCreated,
}

/// Stops authority for an uncertain active write without acknowledging any editor mutation.
///
/// Paths and the failure are projected separately from CommitIssue so this boundary can be
/// checked without introducing filesystem fault constructors into the storage API.
pub(crate) fn uncertain_write(
	session: &mut DesktopSession,
	context: WriteContext<'_>,
	artifact: Option<&Path>,
	reason: &dyn Display,
) -> CommandError {
	let affects_active = context.operation.affects_active();
	if affects_active && let Some(selected) = &mut session.selected {
		selected.fingerprint = None;
	}
	uncertain_result(context, artifact, reason)
}

/// Reports a retained native issue without changing any session or retrying publication.
pub(crate) fn uncertain_result(
	context: WriteContext<'_>,
	artifact: Option<&Path>,
	reason: &dyn Display,
) -> CommandError {
	let affects_active = context.operation.affects_active();
	let backup = context
		.backup
		.map_or(BackupEvidence::NotCreated, |id| BackupEvidence::Verified {
			path: DisplayPath::new(id.path()),
		});
	CommandError::RecoveryRequired(Box::new(receipt(
		context,
		RecoveryOutcome::CommitUncertain,
		affects_active,
		artifact,
		backup,
		reason,
	)))
}

/// Retains preparatory backup evidence while preserving unchanged live-file authority.
pub(crate) fn backup_failure(
	context: WriteContext<'_>,
	failure: NativeBackupError,
) -> CommandError {
	let NativeBackupError::Policy(failure) = failure else {
		return crate::save::write_rejected(failure);
	};
	match *failure {
		BackupError::TargetChanged | BackupError::MissingFile
			if matches!(context.operation, RecoveryOperation::SaveOverwrite) =>
		{
			CommandError::ExternalBankChanged
		}
		BackupError::UncertainBackup { path, issue } => {
			incomplete_backup(context, &path, Some(issue.recovery().path()), issue.error())
		}
		BackupError::Metadata { record, failure } => match failure {
			RecordFailure::Rejected(error) => {
				incomplete_backup(context, record.id().path(), None, &error)
			}
			RecordFailure::Uncertain(issue) => incomplete_backup(
				context,
				record.id().path(),
				Some(issue.recovery().path()),
				issue.error(),
			),
		},
		other => crate::save::write_rejected(other),
	}
}

fn incomplete_backup(
	context: WriteContext<'_>,
	backup: &Path,
	artifact: Option<&Path>,
	reason: &dyn Display,
) -> CommandError {
	CommandError::RecoveryRequired(Box::new(receipt(
		context,
		RecoveryOutcome::LiveUnchanged,
		false,
		artifact,
		BackupEvidence::Unverified {
			path: DisplayPath::new(backup),
		},
		reason,
	)))
}

fn receipt(
	context: WriteContext<'_>,
	outcome: RecoveryOutcome,
	affects_active: bool,
	artifact: Option<&Path>,
	backup: BackupEvidence,
	reason: &dyn Display,
) -> RecoveryReceipt {
	let (reason, reason_truncated) = bounded(&reason.to_string(), MAX_REASON_BYTES);
	RecoveryReceipt {
		operation: context.operation,
		outcome,
		affects_active,
		destination: DisplayPath::new(context.destination),
		recovery: artifact.map_or(RecoveryLocation::Unknown, |path| {
			RecoveryLocation::Recorded {
				path: DisplayPath::new(path),
			}
		}),
		backup,
		reason,
		reason_truncated,
	}
}

fn bounded(value: &str, max_bytes: usize) -> (String, bool) {
	let end = value.floor_char_boundary(value.len().min(max_bytes));
	(value[..end].to_owned(), end < value.len())
}

#[cfg(test)]
mod tests;
