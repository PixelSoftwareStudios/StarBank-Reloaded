use std::path::Path;

use crate::StorageError;

use super::{DirectoryScope, Identity, Observation, absolute_path, terminal_name};

/// An observed Windows volume and file identifier for an ordinary local file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsFileIdentity(Identity);

impl WindowsFileIdentity {
	/// Checks the snapshot storage policy and returns an identity without reading payload bytes.
	///
	/// The identity is useful for rechecking in-session file membership. A filesystem can
	/// reuse identifiers after a file is removed.
	///
	/// # Errors
	///
	/// Rejects paths and file types outside the Windows snapshot policy, or unavailable
	/// metadata or file identifiers.
	pub fn read(path: &Path) -> Result<Self, StorageError> {
		let name = terminal_name(path)?;
		let path = absolute_path(path)?;
		let scope = DirectoryScope::open(&path)?;
		let file = scope.open_file(name)?;
		Ok(Self(Observation::read(&file)?.identity))
	}
}

/// An observed Windows volume and file identifier for an ordinary local directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsDirectoryIdentity(Identity);

impl WindowsDirectoryIdentity {
	/// Reads a directory identity subject to the Windows snapshot directory policy.
	///
	/// # Errors
	///
	/// Rejects unsupported drives, aliases, non-directory paths and unavailable identifiers.
	pub fn read(path: &Path) -> Result<Self, StorageError> {
		let path = absolute_path(path)?;
		Ok(Self(DirectoryScope::open_parent(&path)?.identity))
	}

	/// Returns the serial number of the volume containing the directory.
	#[must_use]
	pub fn volume_serial(self) -> u64 {
		self.0.volume
	}

	/// Returns the volume's 128-bit file identifier for the directory.
	#[must_use]
	pub fn file_id(self) -> [u8; 16] {
		self.0.file_id
	}
}
