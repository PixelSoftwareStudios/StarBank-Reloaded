use std::path::Path;

use sha2::{Digest, Sha256};

use crate::{BackupId, BackupRequest, BackupTimestamp, BankSource, NamingError};

/// Content evidence stored with a backup record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackupIntegrity {
	/// Size of the backed-up contents in bytes.
	pub byte_length: usize,
	/// SHA-256 of the backed-up contents.
	pub sha256: [u8; 32],
}

impl BackupIntegrity {
	/// Measures the exact bytes presented to the backup writer.
	#[must_use]
	pub fn measure(bytes: &[u8]) -> Self {
		Self {
			byte_length: bytes.len(),
			sha256: Sha256::digest(bytes).into(),
		}
	}
}

/// The operation that required a backup snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BackupReason {
	/// Independently requested backup, including each bank in a batch.
	Requested,
	/// User requested Save w/ Backup.
	ExplicitSave,
	/// Automatic backup before overwrite.
	BeforeOverwrite,
	/// Reversal snapshot created before restoring an older version.
	BeforeRestore,
}

/// Metadata for one bank snapshot. Importing metadata requires fresh integrity verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupRecord {
	id: BackupId,
	source: BankSource,
	timestamp: BackupTimestamp,
	sequence: u16,
	reason: BackupReason,
	integrity: BackupIntegrity,
}

impl BackupRecord {
	/// Reconstructs record metadata and derives the snapshot path from its original source.
	///
	/// # Errors
	///
	/// Rejects invalid locations, sequences, and generated filename lengths.
	pub fn from_metadata(
		source: BankSource,
		request: BackupRequest<'_>,
		sequence: u16,
		reason: BackupReason,
		integrity: BackupIntegrity,
	) -> Result<Self, NamingError> {
		let directory = request.directory(&source)?;
		Ok(Self {
			id: BackupId::for_source(&source, directory, request.timestamp, sequence)?,
			source,
			timestamp: request.timestamp,
			sequence,
			reason,
			integrity,
		})
	}

	/// Returns the exact authorized bank path the backup belongs to.
	#[must_use]
	pub fn source(&self) -> &BankSource {
		&self.source
	}

	/// Returns the derived storage ID.
	#[must_use]
	pub fn id(&self) -> &BackupId {
		&self.id
	}

	/// Returns the directory that contains the snapshot, when available.
	#[must_use]
	pub fn directory(&self) -> Option<&Path> {
		self.id.path().parent()
	}

	/// Returns the UTC timestamp.
	#[must_use]
	pub fn timestamp(&self) -> BackupTimestamp {
		self.timestamp
	}

	/// Returns the numeric collision suffix, with zero denoting no suffix.
	#[must_use]
	pub fn sequence(&self) -> u16 {
		self.sequence
	}

	/// Returns why the snapshot was taken.
	#[must_use]
	pub fn reason(&self) -> BackupReason {
		self.reason
	}

	/// Returns stored byte length and SHA-256 digest.
	#[must_use]
	pub fn integrity(&self) -> BackupIntegrity {
		self.integrity
	}
}
