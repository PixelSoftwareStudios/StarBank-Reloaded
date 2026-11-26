use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Write};

use rustix::fs::{self, AtFlags, Mode, OFlags};

use crate::{RecoveryArtifact, StorageError};

use super::destination::{Destination, FileIdentity, ObservedFile, observe};

pub(super) struct Stage<'destination> {
	destination: &'destination Destination,
	name: String,
	pub(super) file: File,
	identity: FileIdentity,
	retain: bool,
}

impl<'destination> Stage<'destination> {
	pub(super) fn create(destination: &'destination Destination) -> Result<Self, StorageError> {
		let mut random = [0; 16];
		getrandom::fill(&mut random).map_err(|error| {
			StorageError::io(
				"generate pending file name",
				io::Error::other(error.to_string()),
			)
		})?;
		let name = format!(".starbank-pending-{:032x}", u128::from_ne_bytes(random));
		let file = File::from(
			fs::openat(
				&destination.dir,
				&name,
				OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
				Mode::RUSR | Mode::WUSR,
			)
			.map_err(|error| StorageError::io("create pending file", error))?,
		);
		let identity = observe(&file)?.identity;
		Ok(Self {
			destination,
			name,
			file,
			identity,
			retain: false,
		})
	}

	pub(super) fn name(&self) -> &OsStr {
		OsStr::new(&self.name)
	}

	pub(super) fn write(&self, bytes: &[u8]) -> Result<(), StorageError> {
		#[cfg(target_os = "macos")]
		super::darwin::ensure_private_stage(&self.file)?;
		(&self.file)
			.write_all(bytes)
			.map_err(|error| StorageError::io("write pending file", error))
	}

	pub(super) fn ensure_bound(&self) -> Result<ObservedFile, StorageError> {
		let observed = observe(&self.file)?;
		self.destination.ensure_file_bound(self.name(), &observed)?;
		Ok(observed)
	}

	pub(super) fn retain(&mut self) -> RecoveryArtifact {
		self.retain = true;
		RecoveryArtifact {
			path: self.destination.sibling(self.name()),
		}
	}
}

impl Drop for Stage<'_> {
	fn drop(&mut self) {
		if self.retain {
			return;
		}
		// Only clean up the file created by this attempt. Failed cleanup leaves the pending file
		// for inspection. The exclusive random name is never reused by another attempt.
		let matches = self
			.destination
			.open_file(self.name(), OFlags::RDONLY)
			.and_then(|file| observe(&file))
			.is_ok_and(|observed| observed.identity == self.identity);
		if matches {
			let _ = fs::unlinkat(&self.destination.dir, self.name(), AtFlags::empty());
		}
	}
}
