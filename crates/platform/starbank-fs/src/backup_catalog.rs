use std::{
	fs, io,
	num::NonZeroUsize,
	path::{Component, Path, PathBuf},
};

use starbank_backup::{
	BackupCatalog, BackupHealth, BackupId, BackupPlacement, BackupRecord, BackupRequest,
	BackupStatus, BackupTimestamp, BankSource, CommitState, Overwrite, OverwriteBackup,
	RetentionPolicy,
};
use time::OffsetDateTime;

use crate::{
	BackupDirectory, CommitIssue, CommitOutcome, CommitReceipt, FileSnapshot, Fingerprint,
	StorageError,
	backup_metadata::{Metadata, backup_directory, is_candidate},
	backup_storage::NativeStorage,
};

const MAX_DIRECTORY_ENTRIES: usize = 10_000;

mod error;
mod snapshot;

pub use error::NativeBackupError;

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
	selected_directory: Option<BackupDirectory>,
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
	managed_directory: Option<BackupDirectory>,
}

impl NativeBackupCatalog {
	/// Constructs an empty catalog with a bounded native read and write size.
	#[must_use]
	pub fn new(max_bytes: NonZeroUsize) -> Self {
		Self {
			catalog: BackupCatalog::new(max_bytes),
			max_bytes: max_bytes.get(),
			managed_directory: None,
		}
	}

	/// Updates the custom managed location while retaining the session backup policy.
	///
	/// Pass this grant's path for managed operations. The grant is rechecked by
	/// native reads and writes, including when the folder changes after selection.
	pub fn set_managed_directory(&mut self, directory: Option<BackupDirectory>) {
		self.managed_directory = directory;
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

	/// Returns older verified records eligible for a separate retention action.
	#[must_use]
	pub fn retention_candidates(
		&self,
		source: &BankSource,
		policy: RetentionPolicy,
	) -> Vec<&BackupRecord> {
		self.catalog.retention_candidates(source, policy)
	}

	/// Verifies the bytes of a previously loaded record under the current location scope.
	///
	/// # Errors
	///
	/// Reports unknown or out-of-scope records, unreadable files, and digest mismatches.
	pub fn verify(&mut self, id: &BackupId) -> Result<(), NativeBackupError> {
		let (record, _) = self
			.catalog
			.get(id)
			.ok_or(starbank_backup::BackupError::UnknownBackup)?;
		let directory = record
			.directory()
			.ok_or(NativeBackupError::InvalidLocation)?;
		let configured = self.managed_directory.as_ref();
		let scope = if Some(directory) == record.source().path().parent() {
			configured.filter(|grant| grant.path() == directory)
		} else {
			managed_scope(configured, BackupPlacement::Managed(directory))?
		};
		let mut storage = NativeStorage::new(scope).with_read_limit(record.integrity().byte_length);
		Ok(self.catalog.verify(&mut storage, id)?)
	}

	/// Loads recorded backups in an authorized location and reports rejected metadata count.
	///
	/// A missing default managed location represents an empty catalog. A custom grant
	/// must still match its directory. Invalid sidecars count as rejected metadata.
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
		let scope = managed_scope(self.managed_directory.as_ref(), placement)?.cloned();
		let mut storage = NativeStorage::new(scope.as_ref());
		if let Some(scope) = &scope {
			let entries = scope
				.entries()?
				.map(|entry| entry.map_err(NativeBackupError::from));
			let rejected =
				self.load_entries(source, placement, verification, &mut storage, entries)?;
			scope.validate()?;
			return Ok(rejected);
		}
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
		let entries = entries.map(|entry| entry.map(|entry| entry.path()).map_err(directory_error));
		self.load_entries(source, placement, verification, &mut storage, entries)
	}

	fn load_entries(
		&mut self,
		source: &BankSource,
		placement: BackupPlacement<'_>,
		verification: BackupVerification,
		storage: &mut NativeStorage<'_>,
		entries: impl Iterator<Item = Result<PathBuf, NativeBackupError>>,
	) -> Result<usize, NativeBackupError> {
		let mut rejected_metadata = 0;
		for (index, path) in entries.enumerate() {
			if index == MAX_DIRECTORY_ENTRIES {
				return Err(NativeBackupError::DirectoryLimit);
			}
			let path = path?;
			if !is_candidate(&path, source) {
				continue;
			}
			let record = match Metadata::read(source, placement, &path, storage) {
				Ok(Some(record)) => record,
				Ok(None) => continue,
				Err(_) => {
					rejected_metadata += 1;
					continue;
				}
			};
			let id = record.id().clone();
			let length = record.integrity().byte_length;
			if self.ensure_record(record).is_err() {
				rejected_metadata += 1;
				continue;
			}
			if verification == BackupVerification::Verify {
				// The policy records the observed health for missing or suspect snapshots.
				let _ = self
					.catalog
					.verify(&mut storage.with_read_limit(length), &id);
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
		let scope = managed_scope(self.managed_directory.as_ref(), placement)?;
		Ok(self
			.catalog
			.create(&mut NativeStorage::new(scope), source, request)?)
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
		let scope = match policy {
			NativeBackupPolicy::FirstInSession(placement)
			| NativeBackupPolicy::EveryOverwrite(placement) => {
				managed_scope(self.managed_directory.as_ref(), placement)?
			}
			NativeBackupPolicy::Explicit | NativeBackupPolicy::Disabled => None,
		};
		let effect = self.catalog.overwrite(
			&mut NativeStorage::new(scope),
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
			selected_directory,
		} = prepared;
		// Confirmed bytes are re-read and checked by the native policy at commit time.
		drop(selected);
		let expected = current.fingerprint().clone();
		drop(current);
		let source = record.source().clone();
		let id = record.id().clone();
		self.ensure_record(record)?;
		let destination = managed_scope(self.managed_directory.as_ref(), reversal)?;
		if let (Some(selected), Some(destination)) = (&selected_directory, destination)
			&& selected.path() == destination.path()
			&& selected != destination
		{
			return Err(StorageError::Conflict.into());
		}
		let mut storage =
			NativeStorage::new(destination).with_selected(selected_directory.as_ref());
		let effect = self.catalog.restore(
			&mut storage,
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

fn managed_scope<'a>(
	configured: Option<&'a BackupDirectory>,
	placement: BackupPlacement<'_>,
) -> Result<Option<&'a BackupDirectory>, NativeBackupError> {
	match (configured, placement) {
		(Some(directory), BackupPlacement::Managed(path)) if path != directory.path() => {
			Err(NativeBackupError::InvalidLocation)
		}
		(Some(directory), BackupPlacement::Managed(_)) => Ok(Some(directory)),
		_ => Ok(None),
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
