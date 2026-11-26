use std::error::Error;
use std::path::Path;

use crate::BackupRecord;

/// An authorized regular-file snapshot and the adapter's exact identity and content token.
#[derive(Debug)]
pub struct Snapshot<F> {
	/// Exact bytes read from the file.
	pub bytes: Vec<u8>,
	/// Token needed to detect an external replacement or edit.
	pub fingerprint: F,
}

/// A read failure at the storage boundary.
#[derive(Debug)]
pub enum ReadFailure<E> {
	/// Storage established that the requested file is absent.
	Missing,
	/// Access, identity or read failed.
	Failed(E),
}

/// The only storage write operations allowed by the policy layer.
#[derive(Debug)]
pub enum StorageIntent<'a, F> {
	/// Atomically publish exclusively. A path collision must leave the existing bytes intact.
	CreateNew,
	/// Compare against the full expected token and replace atomically.
	ReplaceExpected(&'a F),
}

/// A verified or incomplete write result from the storage adapter.
#[derive(Debug)]
pub enum CommitState<R, E, U> {
	/// File verified with the intended exact content. The receipt retains native evidence.
	Verified(R),
	/// Exclusive creation found a collision. Storage left the existing name untouched.
	AlreadyExists,
	/// Storage verified that it left the target unchanged.
	Rejected(E),
	/// A write may have taken effect. The issue retains native recovery evidence.
	Uncertain(U),
}

/// A failure to durably record a verified backup's metadata.
#[derive(Debug)]
pub enum RecordFailure<E, U> {
	/// Metadata publication was rejected without changing existing records.
	Rejected(E),
	/// Metadata publication may have taken effect and requires native verification.
	Uncertain(U),
}

/// Narrow interface for authorized durable storage, supplied by the local platform adapter.
///
/// Implementations must reject unauthorized paths and paths whose identity is ambiguous.
/// The adapter owns native symlink, permissions and metadata policy, synchronization,
/// durability, and recovery artifacts. The policy layer never deletes a backup.
pub trait BackupStorage {
	/// An exact token including file identity and content.
	type Fingerprint: Eq;
	/// Evidence of successful commit and verification.
	type Receipt;
	/// A concrete failure that callers can report or inspect.
	type Error: Error;
	/// A post-commit issue retaining the adapter's recovery evidence.
	type Issue;

	/// Reads a verified file snapshot within the supplied byte limit.
	///
	/// # Errors
	///
	/// Reports an absent file distinctly and rejects unsupported or unsafe reads.
	fn read(
		&mut self,
		path: &Path,
		max_bytes: usize,
	) -> Result<Snapshot<Self::Fingerprint>, ReadFailure<Self::Error>>;

	/// Commits exact bytes, respecting authorization, exclusive creation and expected state.
	///
	/// A verified result requires the adapter to confirm that the published bytes match,
	/// with no silent weaker fallback. Return collision only for exclusive creation.
	fn commit(
		&mut self,
		path: &Path,
		bytes: &[u8],
		intent: StorageIntent<'_, Self::Fingerprint>,
		max_bytes: usize,
	) -> CommitState<Self::Receipt, Self::Error, Self::Issue>;

	/// Durably publishes metadata for a verified snapshot without changing an existing record.
	///
	/// A successful return confirms that a subsequent catalog load can recover the exact
	/// source association, path, size and digest. A failed return must preserve the snapshot
	/// and stop the caller before any bank overwrite.
	///
	/// # Errors
	///
	/// Reports rejected and uncertain metadata publication distinctly.
	fn record_backup(
		&mut self,
		record: &BackupRecord,
	) -> Result<(), RecordFailure<Self::Error, Self::Issue>>;
}
