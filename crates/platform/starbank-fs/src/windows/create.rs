//! Verified Windows create-only transaction on ordinary local NTFS.

use super::{
	Observation,
	publication::{Publication, RenameRequest},
	stage_security::PrivateStageSecurity,
	sync::flush,
};
use crate::{CommitIssue, CommitOutcome, CommitReceipt, RecoveryArtifact, StorageError};
use sha2::{Digest as _, Sha256};
use std::io::Write;
use std::path::Path;
use windows_sys::Win32::{
	Foundation::ERROR_FILE_EXISTS, Storage::FileSystem::FILE_ATTRIBUTE_ARCHIVE,
};

mod leaf;
mod lease;
#[cfg(test)]
mod process_tests;
mod scope;
mod stage;
#[cfg(test)]
mod tests;

use lease::Lease;
use scope::Scope;
use stage::Stage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitPhase {
	Created,
	Written,
	Staged,
	BeforePublication,
	Published,
	FileSynced,
	DirectorySynced,
	HeldVerified,
	WriterClosed,
	Verified,
}

pub(super) fn commit_candidate(path: &Path, bytes: &[u8], max_bytes: usize) -> CommitOutcome {
	try_create(path, bytes, max_bytes, |_| Ok(())).unwrap_or_else(CommitOutcome::RejectedUnchanged)
}

fn try_create(
	path: &Path,
	bytes: &[u8],
	max_bytes: usize,
	checkpoint: impl FnMut(CommitPhase) -> Result<(), StorageError>,
) -> Result<CommitOutcome, StorageError> {
	if bytes.len() > max_bytes {
		return Err(StorageError::TooLarge { max_bytes });
	}
	let scope = Scope::open(path)?;
	try_create_scoped(&scope, bytes, max_bytes, checkpoint)
}

fn try_create_scoped(
	scope: &Scope,
	bytes: &[u8],
	max_bytes: usize,
	mut checkpoint: impl FnMut(CommitPhase) -> Result<(), StorageError>,
) -> Result<CommitOutcome, StorageError> {
	if bytes.len() > max_bytes {
		return Err(StorageError::TooLarge { max_bytes });
	}
	scope.ensure_bound()?;
	let _lease = Lease::acquire(scope.directory.identity)?;
	let request = RenameRequest::new(&scope.directory.parent, &scope.name)?;
	// Qualify actual writable-parent flush before introducing even a private staging file.
	flush(&scope.writable, "sync destination directory before staging")?;
	scope.ensure_bound()?;
	let security = PrivateStageSecurity::current()?;
	let mut stage = Stage::create(&security, &scope.directory, &scope.path)?;
	let digest: [u8; 32] = Sha256::digest(bytes).into();
	let prepared = (|| {
		checkpoint(CommitPhase::Created)?;
		security.verify(&stage.file)?;
		let empty = Observation::read(&stage.file)?;
		if empty.len != 0 || empty.attributes != FILE_ATTRIBUTE_ARCHIVE {
			return Err(StorageError::UnsafeMetadata(
				"unsupported private stage attributes",
			));
		}
		(&stage.file)
			.write_all(bytes)
			.map_err(|error| StorageError::io("write pending file", error))?;
		checkpoint(CommitPhase::Written)?;
		security.verify(&stage.file)?;
		flush(&stage.file, "sync pending file")?;
		let observed = verify_stage(scope, &stage, &security, digest, max_bytes)?;
		scope.ensure_bound()?;
		checkpoint(CommitPhase::Staged)?;
		checkpoint(CommitPhase::BeforePublication)?;
		if verify_stage(scope, &stage, &security, digest, max_bytes)? != observed {
			return Err(StorageError::Conflict);
		}
		scope.ensure_bound()?;
		Ok(observed)
	})();
	let observed = match prepared {
		Ok(observed) => observed,
		Err(error) => return Ok(stage.reject(error)),
	};
	// Never drop-cleanup after entering a syscall that could have published the file.
	stage.attempting_publication();
	match request.publish(&stage.file) {
		Publication::Collision => {
			let collision = (|| {
				let own = verify_stage(scope, &stage, &security, digest, max_bytes)?;
				if own != observed {
					return Err(StorageError::Conflict);
				}
				// A restrictive snapshot prevents data/delete changes while classifying the
				// independent occupant. Equal bytes with a different ID remain a collision.
				let occupant = super::read_snapshot(&scope.path, max_bytes)?;
				if occupant.fingerprint().windows.directory != scope.directory.identity
					|| occupant.fingerprint().windows.file.identity == observed.identity
					|| occupant.fingerprint().windows.file.identity.volume
						!= observed.identity.volume
				{
					return Err(StorageError::Conflict);
				}
				scope.ensure_bound()
			})();
			return Ok(match collision {
				Ok(()) => {
					stage.collision_verified();
					stage.reject(StorageError::io(
						"publish pending file",
						std::io::Error::from_raw_os_error(ERROR_FILE_EXISTS as i32),
					))
				}
				Err(error) => stage.uncertain(error),
			});
		}
		Publication::Indeterminate(error) => return Ok(stage.uncertain(error)),
		Publication::Published => stage.published(),
	}
	let verified = (|| {
		checkpoint(CommitPhase::Published)?;
		flush(&stage.file, "sync published file")?;
		checkpoint(CommitPhase::FileSynced)?;
		scope.ensure_bound()?;
		flush(&scope.writable, "sync destination directory")?;
		checkpoint(CommitPhase::DirectorySynced)?;
		let (file, snapshot) = scope.snapshot(&scope.name, max_bytes)?;
		let published = &snapshot.fingerprint().windows.file;
		if published.attributes != observed.attributes
			|| published.identity != observed.identity
			|| published.len != bytes.len() as u64
			|| snapshot.fingerprint().sha256() != &digest
		{
			return Err(StorageError::Conflict);
		}
		security.verify(&stage.file)?;
		security.verify(&file)?;
		if Observation::read(&stage.file)? != *published || Observation::read(&file)? != *published
		{
			return Err(StorageError::Conflict);
		}
		scope.absent(stage.name())?;
		scope.ensure_bound()?;
		checkpoint(CommitPhase::HeldVerified)?;
		Ok(())
	})();
	if let Err(error) = verified {
		return Ok(stage.uncertain(error));
	}
	let staged_name = stage.name().to_os_string();
	let recovery = RecoveryArtifact {
		path: scope.path.clone(),
	};
	// Windows only guarantees finalized timestamps after the writing handle closes.
	// Keep the held-writer proof above, then capture the receipt under a restrictive reader.
	// https://learn.microsoft.com/windows/win32/sysinfo/file-times
	drop(stage);
	let finalized = (|| {
		checkpoint(CommitPhase::WriterClosed)?;
		let (file, snapshot) = scope.strict_snapshot(&scope.name, max_bytes)?;
		let final_file = &snapshot.fingerprint().windows.file;
		if final_file.identity != observed.identity
			|| final_file.attributes != observed.attributes
			|| final_file.len != bytes.len() as u64
			|| snapshot.fingerprint().sha256() != &digest
		{
			return Err(StorageError::Conflict);
		}
		security.verify(&file)?;
		if Observation::read(&file)? != *final_file {
			return Err(StorageError::Conflict);
		}
		scope.absent(&staged_name)?;
		scope.ensure_bound()?;
		checkpoint(CommitPhase::Verified)?;
		let rebound = scope.directory.open_file(&scope.name)?;
		if Observation::read(&rebound)? != *final_file || Observation::read(&file)? != *final_file {
			return Err(StorageError::Conflict);
		}
		security.verify(&rebound)?;
		security.verify(&file)?;
		scope.ensure_bound()?;
		Ok(snapshot.fingerprint().clone())
	})();
	Ok(match finalized {
		Ok(fingerprint) => CommitOutcome::CommittedVerified(CommitReceipt {
			fingerprint,
			recovery: None,
		}),
		Err(error) => CommitOutcome::CommitUncertain(CommitIssue { error, recovery }),
	})
}

fn verify_stage(
	scope: &Scope,
	stage: &Stage,
	security: &PrivateStageSecurity,
	digest: [u8; 32],
	max_bytes: usize,
) -> Result<Observation, StorageError> {
	let held = Observation::read(&stage.file)?;
	if held.attributes != FILE_ATTRIBUTE_ARCHIVE {
		return Err(StorageError::UnsafeMetadata(
			"unsupported private stage attributes",
		));
	}
	let (named, snapshot) = scope.snapshot(stage.name(), max_bytes)?;
	if held != snapshot.fingerprint().windows.file || snapshot.fingerprint().sha256() != &digest {
		return Err(StorageError::Conflict);
	}
	security.verify(&stage.file)?;
	security.verify(&named)?;
	Ok(held)
}
