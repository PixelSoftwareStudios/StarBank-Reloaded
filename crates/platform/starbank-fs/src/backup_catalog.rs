use std::{
	error::Error,
	fmt, fs, io,
	num::NonZeroUsize,
	path::{Component, Path},
};

use starbank_backup::{
	BackupCatalog, BackupError, BackupHealth, BackupId, BackupPlacement, BackupRecord,
	BackupRequest, BackupStatus, BackupTimestamp, BankSource, CommitState, Overwrite,
	OverwriteBackup,
};
use time::OffsetDateTime;

use crate::{
	CommitIssue, CommitOutcome, CommitReceipt, FileSnapshot, Fingerprint, StorageError,
	backup_metadata::{Metadata, backup_directory, is_candidate, sidecar_path},
	backup_storage::NativeStorage,
	read_snapshot,
};

const MAX_DIRECTORY_ENTRIES: usize = 10_000;

type PolicyError = BackupError<StorageError, CommitIssue>;

/// Verification level for loading previously recorded backups.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupVerification {
	/// Load metadata for browsing without reading snapshot bytes.
	Recorded,
	/// Read each selected snapshot and verify its exact length and digest.
	Verify,
}

/// Policy for backing up a live bank before a native overwrite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBackupPolicy<'a> {
	/// Back up before the first verified protected overwrite in this catalog session.
	FirstInSession(BackupPlacement<'a>),
	/// Back up before each overwrite.
	EveryOverwrite(BackupPlacement<'a>),
	/// Write an adjacent backup regardless of session policy.
	Explicit,
	/// The caller already chose to disable automatic backups.
	Disabled,
}

/// Result of a native overwrite or restore, with recovery evidence where available.
#[derive(Debug)]
pub struct NativeBackupEffect {
	/// Backup created and verified before attempting to write the live bank.
	pub backup: Option<BackupId>,
	/// Native outcome of the guarded live write.
	pub commit: CommitOutcome,
}

/// Scoped metadata, snapshot bytes and current bank state for restore confirmation.
///
/// Callers can validate the selected bank format and display a diff before passing this
/// value to the restore operation. Preparing a restore performs no writes.
#[derive(Debug)]
pub struct PreparedRestore {
	record: BackupRecord,
	current: FileSnapshot,
	selected: FileSnapshot,
}

impl PreparedRestore {
	/// Returns the current authorized snapshot used to guard the restore.
	#[must_use]
	pub fn current(&self) -> &FileSnapshot {
		&self.current
	}

	/// Returns the exact backed-up bytes selected for restore.
	#[must_use]
	pub fn selected(&self) -> &FileSnapshot {
		&self.selected
	}

	/// Returns the verified selection's native backup record.
	#[must_use]
	pub fn record(&self) -> &BackupRecord {
		&self.record
	}
}

/// Verified filesystem-backed bank history and session backup policy.
///
/// Callers retain this catalog across overwrites within an edit session. Each operation
/// uses the selected bank and caller-authorized backup location. Managed directories
/// must be created and selected by the caller.
#[derive(Debug)]
pub struct NativeBackupCatalog {
	catalog: BackupCatalog,
	max_bytes: usize,
}

impl NativeBackupCatalog {
	/// Constructs an empty catalog with a bounded native read and write size.
	#[must_use]
	pub fn new(max_bytes: NonZeroUsize) -> Self {
		Self {
			catalog: BackupCatalog::new(max_bytes),
			max_bytes: max_bytes.get(),
		}
	}

	/// Returns previously loaded and newly created backups, most recent first.
	#[must_use]
	pub fn history(&self, source: &BankSource) -> Vec<(&BackupRecord, BackupHealth)> {
		self.catalog.history(source)
	}

	/// Returns metadata and last observed health for the selected snapshot.
	#[must_use]
	pub fn get(&self, id: &BackupId) -> Option<(&BackupRecord, BackupHealth)> {
		self.catalog.get(id)
	}

	/// Returns the most recent known backup status without accessing disk.
	#[must_use]
	pub fn status(&self, source: &BankSource) -> BackupStatus {
		self.catalog.status(source)
	}

	/// Loads recorded backups in an authorized location and reports rejected metadata count.
	///
	/// A missing managed directory represents an empty catalog location. An individual
	/// malformed sidecar is reported without preventing other records from being loaded.
	///
	/// # Errors
	///
	/// Rejects an unsafe directory, a directory read failure, or an entry count above the limit.
	pub fn load(
		&mut self,
		source: &BankSource,
		placement: BackupPlacement<'_>,
		verification: BackupVerification,
	) -> Result<usize, NativeBackupError> {
		let directory = backup_directory(source, placement)?;
		let entries = match fs::read_dir(directory) {
			Ok(entries) => entries,
			Err(error)
				if matches!(placement, BackupPlacement::Managed(_))
					&& error.kind() == io::ErrorKind::NotFound =>
			{
				return Ok(0);
			}
			Err(error) => return Err(directory_error(error)),
		};
		let mut rejected_metadata = 0;
		for (index, entry) in entries.enumerate() {
			if index == MAX_DIRECTORY_ENTRIES {
				return Err(NativeBackupError::DirectoryLimit);
			}
			let path = entry.map_err(directory_error)?.path();
			if !is_candidate(&path, source) {
				continue;
			}
			let record = match Metadata::read(source, placement, &path) {
				Ok(Some(record)) => record,
				Ok(None) => continue,
				Err(_) => {
					rejected_metadata += 1;
					continue;
				}
			};
			let id = record.id().clone();
			if self.ensure_record(record).is_err() {
				rejected_metadata += 1;
				continue;
			}
			if verification == BackupVerification::Verify {
				// The policy records the observed health for missing or suspect snapshots.
				let _ = self.catalog.verify(&mut NativeStorage, &id);
			}
		}
		Ok(rejected_metadata)
	}

	/// Creates a verified snapshot and durable metadata for the current bank.
	///
	/// # Errors
	///
	/// Reports invalid locations, source reads, size limits, or incomplete publications.
	pub fn create(
		&mut self,
		source: &BankSource,
		placement: BackupPlacement<'_>,
	) -> Result<BackupId, NativeBackupError> {
		let request = BackupRequest {
			timestamp: now()?,
			placement,
		};
		Ok(self.catalog.create(&mut NativeStorage, source, request)?)
	}

	/// Applies the selected backup policy and then attempts a guarded native overwrite.
	///
	/// # Errors
	///
	/// Reports unsupported native replacement, incomplete backup preparation, or changed
	/// current bytes before attempting the live write.
	pub fn overwrite(
		&mut self,
		source: &BankSource,
		expected: &Fingerprint,
		bytes: &[u8],
		policy: NativeBackupPolicy<'_>,
	) -> Result<NativeBackupEffect, NativeBackupError> {
		require_live_replacement()?;
		let backup = match policy {
			NativeBackupPolicy::FirstInSession(placement) => {
				OverwriteBackup::FirstInSession(BackupRequest {
					timestamp: now()?,
					placement,
				})
			}
			NativeBackupPolicy::EveryOverwrite(placement) => {
				OverwriteBackup::EveryOverwrite(BackupRequest {
					timestamp: now()?,
					placement,
				})
			}
			NativeBackupPolicy::Explicit => OverwriteBackup::Explicit(now()?),
			NativeBackupPolicy::Disabled => OverwriteBackup::Disabled,
		};
		let effect = self.catalog.overwrite(
			&mut NativeStorage,
			source,
			Overwrite {
				expected,
				bytes,
				backup,
			},
		)?;
		Ok(NativeBackupEffect {
			backup: effect.backup,
			commit: native_outcome(effect.commit),
		})
	}

	/// Reads a selected snapshot and matches it to a validated sidecar, without writing.
	///
	/// Supply the current file observed when the user selected the restore. The native
	/// restore checks that exact identity and bytes again before writing the live bank.
	///
	/// # Errors
	///
	/// Rejects a mismatched scope, filename, or sidecar, an unreadable snapshot, or invalid
	/// snapshot length or digest.
	pub fn prepare_restore(
		&self,
		source: &BankSource,
		current: FileSnapshot,
		backup: &Path,
		placement: BackupPlacement<'_>,
	) -> Result<PreparedRestore, NativeBackupError> {
		if backup.parent() != Some(backup_directory(source, placement)?) {
			return Err(NativeBackupError::MetadataMismatch);
		}
		let record = Metadata::read(source, placement, &sidecar_path(backup))?
			.ok_or(NativeBackupError::MetadataMismatch)?;
		if record.id().path() != backup {
			return Err(NativeBackupError::MetadataMismatch);
		}
		let selected = read_snapshot(backup, self.max_bytes)?;
		let integrity = record.integrity();
		if selected.fingerprint().sha256() != &integrity.sha256
			|| selected.bytes().len() != integrity.byte_length
		{
			return Err(NativeBackupError::MetadataMismatch);
		}
		Ok(PreparedRestore {
			record,
			current,
			selected,
		})
	}

	/// Creates a verified reversal backup and restores previously confirmed exact bytes.
	///
	/// # Errors
	///
	/// Reports a changed target or backup, unsupported native replacement, or failure
	/// to create and verify the reversal backup before attempting the live write.
	pub fn restore(
		&mut self,
		prepared: PreparedRestore,
		reversal: BackupPlacement<'_>,
	) -> Result<NativeBackupEffect, NativeBackupError> {
		require_live_replacement()?;
		let PreparedRestore {
			record,
			current,
			selected,
		} = prepared;
		// Confirmed bytes are re-read and checked by the native policy at commit time.
		drop(selected);
		let expected = current.fingerprint().clone();
		drop(current);
		let source = record.source().clone();
		let id = record.id().clone();
		self.ensure_record(record)?;
		let effect = self.catalog.restore(
			&mut NativeStorage,
			&id,
			&source,
			&expected,
			BackupRequest {
				timestamp: now()?,
				placement: reversal,
			},
		)?;
		Ok(NativeBackupEffect {
			backup: effect.backup,
			commit: native_outcome(effect.commit),
		})
	}

	fn ensure_record(&mut self, record: BackupRecord) -> Result<(), NativeBackupError> {
		if let Some((existing, _)) = self.catalog.get(record.id()) {
			return if existing == &record {
				Ok(())
			} else {
				Err(NativeBackupError::MetadataMismatch)
			};
		}
		self.catalog
			.import(record)
			.map_err(|_| NativeBackupError::MetadataMismatch)
	}
}

/// Resolves a caller-selected local bank path into its portable backup identity.
///
/// # Errors
///
/// Rejects parent traversal, a path that cannot be made absolute, and nonportable names.
pub fn backup_source(path: &Path) -> Result<BankSource, NativeBackupError> {
	if path
		.components()
		.any(|component| component == Component::ParentDir)
	{
		return Err(NativeBackupError::InvalidSource);
	}
	let absolute = std::path::absolute(path).map_err(|source| StorageError::Io {
		operation: "resolve backup source",
		source,
	})?;
	BankSource::new(absolute).map_err(|_| NativeBackupError::InvalidSource)
}

fn require_live_replacement() -> Result<(), StorageError> {
	if cfg!(target_os = "macos") {
		Ok(())
	} else {
		Err(StorageError::Unsupported(
			"verified native replacement requires local APFS",
		))
	}
}

fn now() -> Result<BackupTimestamp, NativeBackupError> {
	let now = OffsetDateTime::now_utc();
	let (year, month, day) = now.to_calendar_date();
	let (hour, minute, second) = now.to_hms();
	format!(
		"{year:04}{month:02}{day:02}_{hour:02}{minute:02}{second:02}",
		month = u8::from(month)
	)
	.parse()
	.map_err(|_| NativeBackupError::Timestamp)
}

fn native_outcome(effect: CommitState<CommitReceipt, StorageError, CommitIssue>) -> CommitOutcome {
	match effect {
		CommitState::Verified(receipt) => CommitOutcome::CommittedVerified(receipt),
		CommitState::Rejected(error) => CommitOutcome::RejectedUnchanged(error),
		CommitState::Uncertain(issue) => CommitOutcome::CommitUncertain(issue),
		CommitState::AlreadyExists => CommitOutcome::RejectedUnchanged(StorageError::Conflict),
	}
}

fn directory_error(source: io::Error) -> NativeBackupError {
	StorageError::Io {
		operation: "read backup directory",
		source,
	}
	.into()
}

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
