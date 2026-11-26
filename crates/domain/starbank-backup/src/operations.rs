use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use crate::naming::COLLISION_ATTEMPTS;
use crate::{
	BackupCatalog, BackupHealth, BackupId, BackupIntegrity, BackupReason, BackupRecord,
	BackupRequest, BackupStorage, BackupTimestamp, BankSource, CatalogError, CommitState,
	NamingError, ReadFailure, RecordFailure, Snapshot, StorageIntent,
};

/// Backup policy for a live bank overwrite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverwriteBackup<'a> {
	/// Automatically snapshot before this session's first verified protected overwrite.
	FirstInSession(BackupRequest<'a>),
	/// Automatically snapshot before each overwrite.
	EveryOverwrite(BackupRequest<'a>),
	/// User requested Save w/ Backup, which always creates an adjacent snapshot.
	Explicit(BackupTimestamp),
	/// Backup disabled by caller policy.
	Disabled,
}

/// Bytes and commit precondition from the verified document preparation pipeline.
#[derive(Debug)]
pub struct Overwrite<'a, F> {
	/// Full expected current bank identity and content, from the authorized open or confirmation.
	pub expected: &'a F,
	/// Prepared bytes to replace the live bank with.
	pub bytes: &'a [u8],
	/// Backup policy selected by the caller.
	pub backup: OverwriteBackup<'a>,
}

/// Result of a bank write, including any successfully verified preparatory backup.
///
/// Incomplete native outcomes retain the adapter's recovery evidence.
#[derive(Debug)]
pub struct WriteOutcome<R, E, U> {
	/// The verified snapshot created by this request, if any.
	pub backup: Option<BackupId>,
	/// Native result for the live bank write.
	pub commit: CommitState<R, E, U>,
}

/// Outcome of bank overwrite or restore over a given adapter.
pub type StorageOutcome<S> = Result<
	WriteOutcome<
		<S as BackupStorage>::Receipt,
		<S as BackupStorage>::Error,
		<S as BackupStorage>::Issue,
	>,
	BackupError<<S as BackupStorage>::Error, <S as BackupStorage>::Issue>,
>;

type BackupResult<T, S> =
	Result<T, BackupError<<S as BackupStorage>::Error, <S as BackupStorage>::Issue>>;

impl BackupCatalog {
	/// Reads the current bank and creates one never-overwritten backup snapshot.
	///
	/// # Errors
	///
	/// Rejects unsafe reads and destinations, size or collision limits, and failed or uncertain
	/// backup writes. An uncertain write retains the intended backup path for recovery.
	pub fn create<S: BackupStorage>(
		&mut self,
		storage: &mut S,
		source: &BankSource,
		request: BackupRequest<'_>,
	) -> BackupResult<BackupId, S> {
		let snapshot = self.read_snapshot(storage, source.path())?;
		self.capture(
			storage,
			source,
			request,
			BackupReason::Requested,
			&snapshot.bytes,
		)
	}

	/// Verifies the snapshot against its cataloged length and SHA-256.
	///
	/// # Errors
	///
	/// Reports absent records, missing or unreadable snapshots, and integrity mismatches.
	pub fn verify<S: BackupStorage>(
		&mut self,
		storage: &mut S,
		id: &BackupId,
	) -> BackupResult<(), S> {
		self.verified_bytes(storage, id).map(drop)
	}

	/// Backs up current on-disk bytes when required, then attempts a guarded bank replacement.
	///
	/// # Errors
	///
	/// Reports a stale expected token, backup failure, or invalid byte size before attempting
	/// the live write. Live write rejection or uncertainty is returned with the backup ID.
	pub fn overwrite<S: BackupStorage>(
		&mut self,
		storage: &mut S,
		source: &BankSource,
		overwrite: Overwrite<'_, S::Fingerprint>,
	) -> StorageOutcome<S> {
		self.check_length(overwrite.bytes)?;
		let backup_request = match overwrite.backup {
			OverwriteBackup::FirstInSession(request)
				if !self.protected_overwrites.contains(source) =>
			{
				Some((request, BackupReason::BeforeOverwrite))
			}
			OverwriteBackup::EveryOverwrite(request) => {
				Some((request, BackupReason::BeforeOverwrite))
			}
			OverwriteBackup::Explicit(timestamp) => Some((
				BackupRequest::adjacent(timestamp),
				BackupReason::ExplicitSave,
			)),
			OverwriteBackup::FirstInSession(_) | OverwriteBackup::Disabled => None,
		};
		let backup = backup_request
			.map(|(request, reason)| {
				let current = self.read_expected(storage, source, overwrite.expected)?;
				self.capture(storage, source, request, reason, &current.bytes)
			})
			.transpose()?;
		let commit = storage.commit(
			source.path(),
			overwrite.bytes,
			StorageIntent::ReplaceExpected(overwrite.expected),
			self.max_bytes,
		);
		if backup.is_some() && matches!(commit, CommitState::Verified(_)) {
			self.protected_overwrites.insert(source.clone());
		}
		Ok(WriteOutcome { backup, commit })
	}

	/// Restores exact bytes into the same scoped bank after verifying a reversal snapshot.
	///
	/// The expected token must come from the current target shown in the caller's confirmation.
	///
	/// # Errors
	///
	/// Reports unknown or corrupt backup, wrong bank scope, stale target, or failure to create
	/// a reversal snapshot before invoking the live write. Live write rejection or uncertainty
	/// is returned with the reversal snapshot ID.
	pub fn restore<S: BackupStorage>(
		&mut self,
		storage: &mut S,
		id: &BackupId,
		source: &BankSource,
		expected: &S::Fingerprint,
		reversal: BackupRequest<'_>,
	) -> StorageOutcome<S> {
		let (record, _) = self.get(id).ok_or(BackupError::UnknownBackup)?;
		if record.source() != source {
			return Err(BackupError::WrongSource);
		}
		let bytes = self.verified_bytes(storage, id)?;
		let current = self.read_expected(storage, source, expected)?;
		let backup = self.capture(
			storage,
			source,
			reversal,
			BackupReason::BeforeRestore,
			&current.bytes,
		)?;
		let commit = storage.commit(
			source.path(),
			&bytes,
			StorageIntent::ReplaceExpected(expected),
			self.max_bytes,
		);
		Ok(WriteOutcome {
			backup: Some(backup),
			commit,
		})
	}

	fn capture<S: BackupStorage>(
		&mut self,
		storage: &mut S,
		source: &BankSource,
		request: BackupRequest<'_>,
		reason: BackupReason,
		bytes: &[u8],
	) -> BackupResult<BackupId, S> {
		self.ensure_capacity().map_err(BackupError::Catalog)?;
		self.check_length(bytes)?;
		let integrity = BackupIntegrity::measure(bytes);
		for sequence in 0..COLLISION_ATTEMPTS {
			let record =
				BackupRecord::from_metadata(source.clone(), request, sequence, reason, integrity)
					.map_err(BackupError::Naming)?;
			if self.get(record.id()).is_some() {
				continue;
			}
			let path = record.id().path();
			match storage.commit(path, bytes, StorageIntent::CreateNew, self.max_bytes) {
				CommitState::Verified(_) => {
					if let Err(failure) = storage.record_backup(&record) {
						return Err(BackupError::Metadata {
							record: Box::new(record),
							failure,
						});
					}
					return Ok(self.insert_verified(record));
				}
				CommitState::AlreadyExists => {}
				CommitState::Rejected(error) => return Err(BackupError::Storage(error)),
				CommitState::Uncertain(issue) => {
					return Err(BackupError::UncertainBackup {
						path: path.to_owned(),
						issue,
					});
				}
			}
		}
		Err(BackupError::NameCollisionLimit)
	}

	fn verified_bytes<S: BackupStorage>(
		&mut self,
		storage: &mut S,
		id: &BackupId,
	) -> BackupResult<Vec<u8>, S> {
		let (record, _) = self.get(id).ok_or(BackupError::UnknownBackup)?;
		let expected = record.integrity();
		let snapshot = self.read_snapshot(storage, id.path());
		let (result, health) = match snapshot {
			Ok(snapshot) if BackupIntegrity::measure(&snapshot.bytes) == expected => {
				(Ok(snapshot.bytes), BackupHealth::Verified)
			}
			Ok(_) => (
				Err(BackupError::IntegrityMismatch),
				BackupHealth::Unverified,
			),
			Err(BackupError::MissingFile) => (Err(BackupError::MissingFile), BackupHealth::Missing),
			Err(error) => (Err(error), BackupHealth::Unverified),
		};
		self.set_health(id, health);
		result
	}

	fn read_expected<S: BackupStorage>(
		&self,
		storage: &mut S,
		source: &BankSource,
		expected: &S::Fingerprint,
	) -> BackupResult<Snapshot<S::Fingerprint>, S> {
		let current = self.read_snapshot(storage, source.path())?;
		if &current.fingerprint != expected {
			return Err(BackupError::TargetChanged);
		}
		Ok(current)
	}

	fn read_snapshot<S: BackupStorage>(
		&self,
		storage: &mut S,
		path: &Path,
	) -> BackupResult<Snapshot<S::Fingerprint>, S> {
		let snapshot = storage
			.read(path, self.max_bytes)
			.map_err(|failure| match failure {
				ReadFailure::Missing => BackupError::MissingFile,
				ReadFailure::Failed(error) => BackupError::Storage(error),
			})?;
		self.check_length(&snapshot.bytes)?;
		Ok(snapshot)
	}

	fn check_length<E, U>(&self, bytes: &[u8]) -> Result<(), BackupError<E, U>> {
		if bytes.len() > self.max_bytes {
			return Err(BackupError::Catalog(CatalogError::SnapshotTooLarge));
		}
		Ok(())
	}
}

/// Backup or restore precondition failure. A live replacement has not been attempted.
#[derive(Debug)]
#[non_exhaustive]
pub enum BackupError<E, U> {
	/// Invalid bank, timestamp, or backup location.
	Naming(NamingError),
	/// Invalid catalog entry or limits exceeded.
	Catalog(CatalogError),
	/// A known disk operation failed.
	Storage(E),
	/// The requested bank or backup file is absent.
	MissingFile,
	/// The requested backup ID is absent from the catalog.
	UnknownBackup,
	/// The backup belongs to a different bank.
	WrongSource,
	/// The backup failed length or SHA-256 verification.
	IntegrityMismatch,
	/// The current live identity or contents differ from the expected snapshot.
	TargetChanged,
	/// Every bounded exclusive-creation attempt collided with an existing name.
	NameCollisionLimit,
	/// Snapshot bytes were verified but metadata publication requires attention.
	Metadata {
		/// The verified record needed to recover the on-disk snapshot.
		record: Box<BackupRecord>,
		/// Rejected or uncertain metadata publication.
		failure: RecordFailure<E, U>,
	},
	/// The live target is untouched and the snapshot candidate requires recovery.
	UncertainBackup {
		/// Exact path where the attempt may have published a snapshot.
		path: PathBuf,
		/// Native recovery evidence.
		issue: U,
	},
}

impl<E: Display, U> Display for BackupError<E, U> {
	fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Self::Naming(error) => Display::fmt(error, formatter),
			Self::Catalog(error) => Display::fmt(error, formatter),
			Self::Storage(error) => Display::fmt(error, formatter),
			Self::MissingFile => formatter.write_str("bank or backup file is missing"),
			Self::UnknownBackup => formatter.write_str("backup ID is not cataloged"),
			Self::WrongSource => formatter.write_str("backup belongs to another bank"),
			Self::IntegrityMismatch => formatter.write_str("backup failed integrity verification"),
			Self::TargetChanged => formatter.write_str("bank changed after confirmation"),
			Self::Metadata { .. } => {
				formatter.write_str("backup metadata needs verification before any live write")
			}
			Self::NameCollisionLimit => {
				formatter.write_str("backup filename collision limit reached")
			}
			Self::UncertainBackup { .. } => {
				formatter.write_str("backup creation needs verification before any live write")
			}
		}
	}
}

impl<E: Error + 'static, U: fmt::Debug> Error for BackupError<E, U> {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Naming(error) => Some(error),
			Self::Catalog(error) => Some(error),
			Self::Storage(error)
			| Self::Metadata {
				failure: RecordFailure::Rejected(error),
				..
			} => Some(error),
			_ => None,
		}
	}
}
