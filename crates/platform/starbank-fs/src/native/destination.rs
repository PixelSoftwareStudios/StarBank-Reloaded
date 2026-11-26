use std::ffi::{OsStr, OsString};
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use sha2::{Digest, Sha256};

use crate::{FileSnapshot, Fingerprint, StorageError};

const OPEN_DIRECTORY: OFlags = OFlags::RDONLY
	.union(OFlags::DIRECTORY)
	.union(OFlags::NOFOLLOW)
	.union(OFlags::CLOEXEC);
const OPEN_FILE: OFlags = OFlags::NOFOLLOW
	.union(OFlags::CLOEXEC)
	.union(OFlags::NOCTTY)
	.union(OFlags::NONBLOCK);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
	device: u64,
	inode: u64,
}

impl FileIdentity {
	pub(super) fn new(metadata: &Metadata) -> Self {
		Self {
			device: metadata.dev(),
			inode: metadata.ino(),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedFile {
	pub(super) identity: FileIdentity,
	len: u64,
	mode: u32,
	uid: u32,
	gid: u32,
	modified: (i64, i64),
	changed: (i64, i64),
}

impl ObservedFile {
	pub(super) fn new(metadata: &Metadata) -> Result<Self, StorageError> {
		if !metadata.is_file() || metadata.nlink() != 1 {
			return Err(StorageError::UnsafePath(
				"only regular files with one hard link are supported",
			));
		}
		Ok(Self {
			identity: FileIdentity::new(metadata),
			len: metadata.len(),
			mode: metadata.mode(),
			uid: metadata.uid(),
			gid: metadata.gid(),
			modified: (metadata.mtime(), metadata.mtime_nsec()),
			changed: (metadata.ctime(), metadata.ctime_nsec()),
		})
	}

	pub(super) fn same_security(&self, other: &Self) -> bool {
		self.mode == other.mode && self.uid == other.uid && self.gid == other.gid
	}
}

pub(super) struct Destination {
	pub(super) dir: File,
	pub(super) name: OsString,
	parent: PathBuf,
	identity: FileIdentity,
}

impl Destination {
	pub(super) fn open(path: &Path) -> Result<Self, StorageError> {
		let name = path
			.file_name()
			.ok_or(StorageError::UnsafePath("missing regular file name"))?
			.to_os_string();
		if !path
			.as_os_str()
			.as_bytes()
			.rsplit(|byte| *byte == b'/')
			.next()
			.is_some_and(|leaf| leaf == name.as_bytes())
		{
			return Err(StorageError::UnsafePath("path must end with the file name"));
		}
		let parent = path
			.parent()
			.ok_or(StorageError::UnsafePath("missing parent directory"))?
			.to_path_buf();
		let parent = if parent.is_absolute() {
			parent
		} else {
			std::env::current_dir()
				.map_err(|error| StorageError::io("resolve current directory", error))?
				.join(parent)
		};
		let dir = open_directory(&parent)?;
		let identity = directory_identity(&dir)?;
		Ok(Self {
			dir,
			name,
			parent,
			identity,
		})
	}

	pub(super) fn ensure_bound(&self) -> Result<(), StorageError> {
		let current = open_directory(&self.parent)?;
		if directory_identity(&current)? != self.identity {
			return Err(StorageError::Conflict);
		}
		Ok(())
	}

	pub(super) fn sibling(&self, name: &OsStr) -> PathBuf {
		self.parent.join(name)
	}

	pub(super) fn open_file(&self, name: &OsStr, access: OFlags) -> Result<File, StorageError> {
		let stat = fs::statat(&self.dir, name, AtFlags::SYMLINK_NOFOLLOW)
			.map_err(|error| StorageError::io("inspect regular file", error))?;
		if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
			return Err(StorageError::UnsafePath("only regular files are supported"));
		}
		fs::openat(&self.dir, name, OPEN_FILE | access, Mode::empty())
			.map(File::from)
			.map_err(|error| StorageError::io("open regular file", error))
	}

	pub(super) fn read_named(
		&self,
		name: &OsStr,
		max_bytes: usize,
	) -> Result<FileSnapshot, StorageError> {
		let file = self.open_file(name, OFlags::RDONLY)?;
		self.read_bound(&file, name, max_bytes)
	}

	pub(super) fn read_bound(
		&self,
		file: &File,
		name: &OsStr,
		max_bytes: usize,
	) -> Result<FileSnapshot, StorageError> {
		let before = observe(file)?;
		let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
		if before.len > limit {
			return Err(StorageError::TooLarge { max_bytes });
		}
		let mut bytes = Vec::new();
		let observed_len =
			usize::try_from(before.len).map_err(|_| StorageError::TooLarge { max_bytes })?;
		bytes.try_reserve_exact(observed_len).map_err(|error| {
			StorageError::io(
				"reserve snapshot buffer",
				io::Error::new(io::ErrorKind::OutOfMemory, error),
			)
		})?;
		file.take(limit.saturating_add(1))
			.read_to_end(&mut bytes)
			.map_err(|error| StorageError::io("read regular file", error))?;
		if bytes.len() > max_bytes {
			return Err(StorageError::TooLarge { max_bytes });
		}
		self.ensure_file_bound(name, &before)?;
		if observe(file)? != before || bytes.len() != observed_len {
			return Err(StorageError::Conflict);
		}
		let fingerprint = Fingerprint {
			observed: before,
			directory: self.identity,
			name: name.to_os_string(),
			sha256: digest(&bytes),
		};
		Ok(FileSnapshot { bytes, fingerprint })
	}

	pub(super) fn ensure_file_bound(
		&self,
		name: &OsStr,
		expected: &ObservedFile,
	) -> Result<(), StorageError> {
		let named = self.open_file(name, OFlags::RDONLY)?;
		if observe(&named)? != *expected {
			return Err(StorageError::Conflict);
		}
		Ok(())
	}
}

/// Reads a regular single-link file without following symlinks.
///
/// # Errors
///
/// Returns a storage error for unsafe paths, excessive input, concurrent changes, or external I/O
/// failures. The input is resolved relative to the current directory when it is not absolute.
pub fn read_snapshot(path: &Path, max_bytes: usize) -> Result<FileSnapshot, StorageError> {
	let destination = Destination::open(path)?;
	let snapshot = destination.read_named(&destination.name, max_bytes)?;
	destination.ensure_bound()?;
	Ok(snapshot)
}

pub(super) fn observe(file: &File) -> Result<ObservedFile, StorageError> {
	let metadata = file
		.metadata()
		.map_err(|error| StorageError::io("stat file", error))?;
	ObservedFile::new(&metadata)
}

pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
	Sha256::digest(bytes).into()
}

fn open_directory(path: &Path) -> Result<File, StorageError> {
	let anchor = if path.is_absolute() { "/" } else { "." };
	let mut dir = File::from(
		fs::open(anchor, OPEN_DIRECTORY, Mode::empty())
			.map_err(|error| StorageError::io("open path anchor", error))?,
	);
	for component in path.components() {
		match component {
			Component::RootDir | Component::CurDir => {}
			Component::Normal(name) => {
				dir = File::from(
					fs::openat(&dir, name, OPEN_DIRECTORY, Mode::empty())
						.map_err(|error| StorageError::io("open parent directory", error))?,
				);
			}
			Component::ParentDir | Component::Prefix(_) => {
				return Err(StorageError::UnsafePath("parent traversal is unsupported"));
			}
		}
	}
	Ok(dir)
}

fn directory_identity(dir: &File) -> Result<FileIdentity, StorageError> {
	dir.metadata()
		.map(|metadata| FileIdentity::new(&metadata))
		.map_err(|error| StorageError::io("stat parent directory", error))
}
