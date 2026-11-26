use std::path::{Path, PathBuf};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{
	ffi::OsStr, fs::File, os::unix::ffi::OsStrExt, os::unix::fs::MetadataExt, time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use crate::{CommitIntent, CommitOutcome, FileSnapshot, StorageError};

/// Native identity of an explicitly selected managed backup directory.
///
/// A serialized grant is suitable for trusted local settings. Validate the current
/// directory before offering a persisted grant for use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupDirectory {
	#[cfg_attr(
		any(target_os = "linux", target_os = "macos"),
		serde(with = "native_path")
	)]
	path: PathBuf,
	identity: DirectoryIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryIdentity {
	device: u64,
	inode: u64,
	birth_nanos: Option<i128>,
}

type DirectoryEntries<'a> = Box<dyn Iterator<Item = Result<PathBuf, StorageError>> + 'a>;

impl BackupDirectory {
	/// Captures the observable identity of an ordinary absolute directory.
	///
	/// # Errors
	///
	/// Rejects relative paths, symlinks, unsafe or unsupported filesystems and I/O failures.
	pub fn capture(path: &Path) -> Result<Self, StorageError> {
		#[cfg(any(target_os = "linux", target_os = "macos"))]
		{
			let directory = open_qualified(path)?;
			let grant = Self {
				path: path.to_owned(),
				identity: DirectoryIdentity::of(&directory)?,
			};
			grant.validate()?;
			Ok(grant)
		}
		#[cfg(not(any(target_os = "linux", target_os = "macos")))]
		{
			let _ = path;
			Err(unsupported())
		}
	}

	/// Returns the native folder selected when this grant was captured.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Reopens the path without following links and matches the captured directory.
	///
	/// # Errors
	///
	/// Reports replacement, unsafe paths, unsupported filesystems and I/O failures.
	pub fn validate(&self) -> Result<(), StorageError> {
		#[cfg(any(target_os = "linux", target_os = "macos"))]
		return self.open().map(drop);
		#[cfg(not(any(target_os = "linux", target_os = "macos")))]
		Err(unsupported())
	}

	#[cfg(any(target_os = "linux", target_os = "macos"))]
	pub(crate) fn open(&self) -> Result<File, StorageError> {
		let directory = open_qualified(&self.path)?;
		if DirectoryIdentity::of(&directory)? != self.identity {
			return Err(StorageError::Conflict);
		}
		Ok(directory)
	}

	pub(super) fn entries(&self) -> Result<DirectoryEntries<'_>, StorageError> {
		#[cfg(any(target_os = "linux", target_os = "macos"))]
		{
			let dir = rustix::fs::Dir::read_from(self.open()?)
				.map_err(|error| StorageError::io("read managed backup directory", error))?;
			Ok(Box::new(dir.filter_map(|entry| match entry {
				Ok(entry) => match entry.file_name().to_bytes() {
					b"." | b".." => None,
					name => Some(Ok(self.path.join(OsStr::from_bytes(name)))),
				},
				Err(error) => Some(Err(StorageError::io("read managed backup entry", error))),
			})))
		}
		#[cfg(not(any(target_os = "linux", target_os = "macos")))]
		Err(unsupported())
	}

	pub(super) fn read(&self, path: &Path, max_bytes: usize) -> Result<FileSnapshot, StorageError> {
		#[cfg(any(target_os = "linux", target_os = "macos"))]
		return crate::native::read_scoped(path, max_bytes, Some(self));
		#[cfg(not(any(target_os = "linux", target_os = "macos")))]
		{
			let _ = (path, max_bytes);
			Err(unsupported())
		}
	}

	pub(super) fn commit(
		&self,
		path: &Path,
		bytes: &[u8],
		intent: CommitIntent<'_>,
		max_bytes: usize,
	) -> CommitOutcome {
		#[cfg(any(target_os = "linux", target_os = "macos"))]
		return crate::native::commit_scoped(path, bytes, intent, max_bytes, Some(self));
		#[cfg(not(any(target_os = "linux", target_os = "macos")))]
		{
			let _ = (path, bytes, intent, max_bytes);
			CommitOutcome::RejectedUnchanged(unsupported())
		}
	}
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl DirectoryIdentity {
	fn of(directory: &File) -> Result<Self, StorageError> {
		let metadata = directory
			.metadata()
			.map_err(|error| StorageError::io("stat managed backup directory", error))?;
		Ok(Self {
			device: metadata.dev(),
			inode: metadata.ino(),
			birth_nanos: metadata.created().ok().map(|created| {
				match created.duration_since(UNIX_EPOCH) {
					Ok(duration) => duration.as_nanos().cast_signed(),
					Err(before) => -before.duration().as_nanos().cast_signed(),
				}
			}),
		})
	}
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_qualified(path: &Path) -> Result<File, StorageError> {
	if !path.is_absolute() {
		return Err(StorageError::UnsafePath(
			"managed backup directory requires an absolute path",
		));
	}
	let directory = crate::native::open_directory(path)?;
	#[cfg(target_os = "macos")]
	crate::native::require_local_apfs(&directory)?;
	Ok(directory)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn unsupported() -> StorageError {
	StorageError::Unsupported("custom managed backup directories are unavailable")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native_path {
	use std::{
		ffi::OsString,
		os::unix::ffi::OsStringExt,
		path::{Path, PathBuf},
	};

	use serde::{Deserialize, Deserializer, Serializer};

	pub(super) fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
		use std::os::unix::ffi::OsStrExt as _;
		serializer.serialize_bytes(path.as_os_str().as_bytes())
	}

	pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<PathBuf, D::Error> {
		let units = Vec::<u8>::deserialize(input)?;
		Ok(OsString::from_vec(units).into())
	}
}
