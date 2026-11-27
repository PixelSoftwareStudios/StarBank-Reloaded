//! Owns only this transaction's private file and disposes it by handle before publication.

use super::super::{DirectoryScope, stage_security::PrivateStageSecurity};
use crate::{CommitIssue, CommitOutcome, RecoveryArtifact, StorageError};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
	FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
};

const RANDOM_BYTES: usize = 16;
const PENDING_PREFIX: &str = ".starbank-pending-";

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
	Unpublished,
	Attempted,
	Published,
	Retained,
}

pub(super) struct Stage {
	pub(super) file: File,
	name: OsString,
	staged_path: PathBuf,
	destination: PathBuf,
	state: State,
}

impl Stage {
	pub(super) fn create(
		security: &PrivateStageSecurity,
		scope: &DirectoryScope,
		destination: &Path,
	) -> Result<Self, StorageError> {
		let mut random = [0; RANDOM_BYTES];
		getrandom::fill(&mut random).map_err(|error| {
			StorageError::io(
				"generate pending file name",
				io::Error::other(error.to_string()),
			)
		})?;
		let name = OsString::from(format!("{PENDING_PREFIX}{}", hex::encode(random)));
		let file = security.create(scope, &name)?;
		// From this point the guard exists before any fallible identity/security inspection.
		Ok(Self {
			file,
			staged_path: destination.with_file_name(&name),
			name,
			destination: destination.to_path_buf(),
			state: State::Unpublished,
		})
	}
	pub(super) fn name(&self) -> &OsStr {
		&self.name
	}
	pub(super) fn attempting_publication(&mut self) {
		self.state = State::Attempted;
	}
	pub(super) fn published(&mut self) {
		self.state = State::Published;
	}
	pub(super) fn collision_verified(&mut self) {
		self.state = State::Unpublished;
	}

	pub(super) fn uncertain(&mut self, error: StorageError) -> CommitOutcome {
		let path = if self.state == State::Published {
			&self.destination
		} else {
			&self.staged_path
		};
		let recovery = RecoveryArtifact { path: path.clone() };
		self.state = State::Retained;
		CommitOutcome::CommitUncertain(CommitIssue { error, recovery })
	}

	pub(super) fn reject(&mut self, error: StorageError) -> CommitOutcome {
		if let Err(cleanup) = dispose(&self.file) {
			return self.uncertain(StorageError::io(
				"cleanup rejected Windows stage",
				io::Error::other(CleanupFailure {
					original: error,
					cleanup,
				}),
			));
		}
		self.state = State::Retained;
		CommitOutcome::RejectedUnchanged(error)
	}
}

impl Drop for Stage {
	fn drop(&mut self) {
		if self.state == State::Unpublished {
			// Panic unwinding before the publication call still disposes only the created
			// object. Failure leaves a private stage; it never falls back to path deletion.
			let _ = dispose(&self.file);
		}
	}
}

fn dispose(file: &File) -> Result<(), StorageError> {
	let information = FILE_DISPOSITION_INFO { DeleteFile: true };
	// SAFETY: the live exclusive-created file retains DELETE access. The initialized SDK
	// structure and exact information class only mark this owned object for deletion on close.
	let result = unsafe {
		SetFileInformationByHandle(
			file.as_raw_handle(),
			FileDispositionInfo,
			(&raw const information).cast(),
			size_of::<FILE_DISPOSITION_INFO>() as u32,
		)
	};
	if result == 0 {
		return Err(StorageError::io(
			"dispose unpublished pending file",
			io::Error::last_os_error(),
		));
	}
	Ok(())
}

#[derive(Debug)]
struct CleanupFailure {
	original: StorageError,
	cleanup: StorageError,
}

impl std::fmt::Display for CleanupFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{}; unpublished stage cleanup also failed: {}",
			self.original, self.cleanup
		)
	}
}
impl std::error::Error for CleanupFailure {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		Some(&self.cleanup)
	}
}
