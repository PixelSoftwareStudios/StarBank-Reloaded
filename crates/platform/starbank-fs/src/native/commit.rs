use std::fs::File;
use std::path::Path;

#[cfg(target_os = "macos")]
use rustix::fs::OFlags;
use rustix::fs::{self, FlockOperation, RenameFlags};
use rustix::io::Errno;

use crate::{CommitIntent, CommitIssue, CommitOutcome, CommitReceipt, Fingerprint, StorageError};

#[cfg(target_os = "macos")]
use super::darwin;
use super::destination::{Destination, ObservedFile, digest};
use super::stage::Stage;

/// Checks the current native replacement scope without staging or publishing bytes.
///
/// Availability can change after the check. Callers must still obtain a verified commit
/// against their captured fingerprint.
///
/// # Errors
///
/// Rejects unsupported filesystems, unsafe targets or unsupported metadata.
#[cfg(target_os = "macos")]
pub fn check_replace_scope(path: &Path) -> Result<(), StorageError> {
	let destination = Destination::open(path)?;
	darwin::require_local_apfs(&destination.dir)?;
	let file = destination.open_file(&destination.name, OFlags::RDWR)?;
	let observed = super::destination::observe(&file)?;
	darwin::require_writable_metadata(&file)?;
	destination.ensure_file_bound(&destination.name, &observed)?;
	destination.ensure_bound()
}

/// Checks native replacement availability.
///
/// # Errors
///
/// Linux requires a qualified overwrite adapter before replacement is available.
#[cfg(target_os = "linux")]
pub fn check_replace_scope(_path: &Path) -> Result<(), StorageError> {
	Err(StorageError::Unsupported("Linux overwrite is unavailable"))
}

/// Publishes previously validated local bytes under the caller's explicit intent.
///
/// macOS publication is restricted to local APFS. Linux supports exclusive creation when the
/// filesystem supports the native rename operation. A busy directory lease rejects the attempt.
/// Callers manage backup policy before overwrite.
/// An external writer can race between the last check and publication. A displaced file that cannot
/// be verified against the open snapshot leaves a recovery artifact and an uncertain outcome.
pub fn commit(
	path: &Path,
	bytes: &[u8],
	intent: CommitIntent<'_>,
	max_bytes: usize,
) -> CommitOutcome {
	match try_commit(path, bytes, intent, max_bytes, |_| Ok(())) {
		Ok(outcome) => outcome,
		Err(error) => CommitOutcome::RejectedUnchanged(error),
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitPhase {
	Staged,
	Published,
	DirectorySynced,
}

fn try_commit(
	path: &Path,
	bytes: &[u8],
	intent: CommitIntent<'_>,
	max_bytes: usize,
	mut checkpoint: impl FnMut(CommitPhase) -> Result<(), StorageError>,
) -> Result<CommitOutcome, StorageError> {
	if bytes.len() > max_bytes {
		return Err(StorageError::TooLarge { max_bytes });
	}
	#[cfg(target_os = "linux")]
	if matches!(intent, CommitIntent::ReplaceExpected(_)) {
		return Err(StorageError::Unsupported("Linux overwrite is unavailable"));
	}

	let destination = Destination::open(path)?;
	#[cfg(target_os = "macos")]
	darwin::require_local_apfs(&destination.dir)?;
	// The held directory descriptor serializes cooperating writers in this directory, even
	// across processes or differently spelled file names on case-insensitive filesystems.
	fs::flock(&destination.dir, FlockOperation::NonBlockingLockExclusive)
		.map_err(|error| StorageError::io("lock destination directory", error))?;
	let mut stage = Stage::create(&destination)?;
	let digest = digest(bytes);
	stage.write(bytes)?;

	#[cfg(target_os = "macos")]
	let original = match intent {
		CommitIntent::CreateNew => None,
		CommitIntent::ReplaceExpected(expected) => Some(Original::prepare(
			&destination,
			&stage,
			expected,
			max_bytes,
		)?),
	};

	sync(&stage.file, "sync pending file")?;
	let staged = stage.ensure_bound()?;
	destination.ensure_bound()?;
	#[cfg(target_os = "macos")]
	if let Some(original) = &original {
		original.ensure_prepared(&destination, &stage, &staged, max_bytes)?;
	}
	if stage.ensure_bound()? != staged {
		return Err(StorageError::Conflict);
	}

	let flags = match intent {
		CommitIntent::CreateNew => RenameFlags::NOREPLACE,
		CommitIntent::ReplaceExpected(_) => RenameFlags::EXCHANGE,
	};
	#[cfg(target_os = "macos")]
	let flags = darwin::publication_flags(flags);
	checkpoint(CommitPhase::Staged)?;
	let publish = fs::renameat_with(
		&destination.dir,
		stage.name(),
		&destination.dir,
		&destination.name,
		flags,
	);
	if let Err(error) = publish {
		let failure = StorageError::io("publish pending file", error);
		if matches!((intent, error), (CommitIntent::CreateNew, Errno::EXIST))
			|| error == Errno::NOSYS
			|| error == Errno::NOTSUP
		{
			return Err(failure);
		}
		return Ok(uncertain(&mut stage, failure));
	}

	let verified = (|| {
		checkpoint(CommitPhase::Published)?;
		sync(&destination.dir, "sync destination directory")?;
		checkpoint(CommitPhase::DirectorySynced)?;
		let fingerprint = verify(&destination, &staged, digest, max_bytes)?;
		#[cfg(target_os = "macos")]
		if let Some(original) = &original {
			original.verify_replacement(&destination, &stage, &fingerprint, max_bytes)?;
		}
		Ok(fingerprint)
	})();
	Ok(match verified {
		Ok(fingerprint) => {
			let recovery =
				matches!(intent, CommitIntent::ReplaceExpected(_)).then(|| stage.retain());
			CommitOutcome::CommittedVerified(CommitReceipt {
				fingerprint,
				recovery,
			})
		}
		Err(error) => uncertain(&mut stage, error),
	})
}

fn uncertain(stage: &mut Stage<'_>, error: StorageError) -> CommitOutcome {
	CommitOutcome::CommitUncertain(CommitIssue {
		error,
		recovery: stage.retain(),
	})
}

fn verify(
	destination: &Destination,
	staged: &ObservedFile,
	sha256: [u8; 32],
	max_bytes: usize,
) -> Result<Fingerprint, StorageError> {
	let snapshot = destination.read_named(&destination.name, max_bytes)?;
	let fingerprint = snapshot.fingerprint;
	if fingerprint.observed.identity != staged.identity
		|| fingerprint.sha256 != sha256
		|| !fingerprint.observed.same_security(staged)
	{
		return Err(StorageError::Conflict);
	}
	destination.ensure_bound()?;
	Ok(fingerprint)
}

fn sync(file: &File, operation: &'static str) -> Result<(), StorageError> {
	fs::fsync(file).map_err(|error| StorageError::io(operation, error))?;
	#[cfg(target_os = "macos")]
	fs::fcntl_fullfsync(file).map_err(|error| StorageError::io(operation, error))?;
	Ok(())
}

#[cfg(target_os = "macos")]
struct Original<'expected> {
	expected: &'expected Fingerprint,
	security: darwin::Security,
}

#[cfg(target_os = "macos")]
impl<'expected> Original<'expected> {
	fn prepare(
		destination: &Destination,
		stage: &Stage<'_>,
		expected: &'expected Fingerprint,
		max_bytes: usize,
	) -> Result<Self, StorageError> {
		// The overwrite must have write access to the original even when directory permissions
		// would allow a name replacement. The existing file is never opened with truncation.
		let file = destination.open_file(&destination.name, OFlags::RDWR)?;
		let snapshot = destination.read_bound(&file, &destination.name, max_bytes)?;
		if snapshot.fingerprint() != expected {
			return Err(StorageError::Conflict);
		}
		let security = darwin::metadata(&file)?;
		darwin::copy_metadata(&file, &stage.file)?;
		Ok(Self { expected, security })
	}

	fn ensure_prepared(
		&self,
		destination: &Destination,
		stage: &Stage<'_>,
		staged: &ObservedFile,
		max_bytes: usize,
	) -> Result<(), StorageError> {
		if !staged.same_security(&self.expected.observed)
			|| darwin::metadata(&stage.file)? != self.security
		{
			return Err(StorageError::UnsafeMetadata(
				"pending file metadata differs",
			));
		}
		let current = destination.open_file(&destination.name, OFlags::RDWR)?;
		let snapshot = destination.read_bound(&current, &destination.name, max_bytes)?;
		if snapshot.fingerprint() != self.expected || darwin::metadata(&current)? != self.security {
			return Err(StorageError::Conflict);
		}
		destination.ensure_file_bound(&destination.name, &self.expected.observed)?;
		Ok(())
	}

	fn verify_replacement(
		&self,
		destination: &Destination,
		stage: &Stage<'_>,
		published: &Fingerprint,
		max_bytes: usize,
	) -> Result<(), StorageError> {
		let old = destination.open_file(stage.name(), OFlags::RDONLY)?;
		let displaced = destination.read_bound(&old, stage.name(), max_bytes)?;
		let previous = displaced.fingerprint();
		if previous.observed.identity != self.expected.observed.identity
			|| previous.sha256 != self.expected.sha256
			|| !previous.observed.same_security(&self.expected.observed)
			|| darwin::metadata(&old)? != self.security
			|| !published.observed.same_security(&self.expected.observed)
			|| darwin::metadata(&stage.file)? != self.security
		{
			return Err(StorageError::Conflict);
		}
		destination.ensure_file_bound(stage.name(), &previous.observed)?;
		destination.ensure_file_bound(&destination.name, &published.observed)?;
		destination.ensure_bound()
	}
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
