use std::path::Path;

#[cfg(not(windows))]
use crate::FileSnapshot;
use crate::{CommitIntent, CommitOutcome, StorageError};

#[cfg(not(windows))]
const UNSUPPORTED_PLATFORM: &str = "native storage is unavailable on this platform";
#[cfg(windows)]
const UNSUPPORTED_PLATFORM: &str = "verified native writes are unavailable on Windows";

/// Returns an unsupported-platform error.
///
/// # Errors
///
/// Always returns [`StorageError::Unsupported`] on this platform.
#[cfg(not(windows))]
pub fn read_snapshot(_path: &Path, _max_bytes: usize) -> Result<FileSnapshot, StorageError> {
	Err(StorageError::Unsupported(UNSUPPORTED_PLATFORM))
}

/// Checks native replacement availability.
///
/// # Errors
///
/// Always returns an unsupported-platform error.
pub fn check_replace_scope(_path: &Path) -> Result<(), StorageError> {
	Err(StorageError::Unsupported(UNSUPPORTED_PLATFORM))
}

/// Rejects the commit before opening a file on this platform.
pub fn commit(
	_path: &Path,
	_bytes: &[u8],
	_intent: CommitIntent<'_>,
	_max_bytes: usize,
) -> CommitOutcome {
	CommitOutcome::RejectedUnchanged(StorageError::Unsupported(UNSUPPORTED_PLATFORM))
}
