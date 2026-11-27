//! Windows read-only snapshots of ordinary local files.
#![allow(
	unsafe_code,
	reason = "Win32 handle operations are isolated in this platform boundary"
)]

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::mem::size_of;
use std::os::windows::{
	ffi::OsStrExt,
	fs::{MetadataExt, OpenOptionsExt},
	io::AsRawHandle,
};
use std::path::{Component, Path, PathBuf, Prefix};

use cap_primitives::fs::{OpenOptions as RelativeOpenOptions, OpenOptionsExt as _};
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
	Storage::FileSystem::{
		FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE,
		FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
		FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
		FILE_FLAG_OPEN_NO_RECALL, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
		FILE_INFO_BY_HANDLE_CLASS, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
		FILE_STANDARD_INFO, FILE_TRAVERSE, FILE_TYPE_DISK, FileBasicInfo, FileIdInfo,
		FileStandardInfo, GetDriveTypeW, GetFileInformationByHandleEx, GetFileType,
		GetFinalPathNameByHandleW, SYNCHRONIZE, VOLUME_NAME_GUID,
	},
	System::WindowsProgramming::{DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE},
};

use crate::{FileSnapshot, Fingerprint, StorageError};

#[cfg(test)]
mod create;
mod identity;
#[cfg(test)]
mod publication;
#[cfg(test)]
mod stage_security;
#[cfg(test)]
mod sync;

pub use identity::{WindowsDirectoryIdentity, WindowsFileIdentity};

const SPECIAL_ATTRIBUTES: u32 = FILE_ATTRIBUTE_DEVICE
	| FILE_ATTRIBUTE_OFFLINE
	| FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
	| FILE_ATTRIBUTE_RECALL_ON_OPEN
	| FILE_ATTRIBUTE_REPARSE_POINT;
const DIRECTORY_ACCESS: u32 = FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE;
const LOCAL_ROOT_GUID_CAPACITY: u32 = 64;
const NO_REPARSE_OR_RECALL: u32 = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedPath {
	file: Observation,
	directory: Identity,
	name: OsString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
	volume: u64,
	file_id: [u8; 16],
}

impl Identity {
	fn read(file: &File) -> Result<Self, StorageError> {
		// SAFETY: FileIdInfo initializes a FILE_ID_INFO whose fields accept all bit patterns.
		let data = unsafe { query::<FILE_ID_INFO>(file, FileIdInfo) }?;
		if data.FileId.Identifier == [0; 16] {
			return Err(StorageError::Unsupported(
				"the filesystem did not return a file ID",
			));
		}
		Ok(Self {
			volume: data.VolumeSerialNumber,
			file_id: data.FileId.Identifier,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
	identity: Identity,
	len: u64,
	attributes: u32,
	created: i64,
	modified: i64,
	changed: i64,
}

impl Observation {
	fn read(file: &File) -> Result<Self, StorageError> {
		// SAFETY: the descriptor is borrowed from a live File.
		if unsafe { GetFileType(file.as_raw_handle()) } != FILE_TYPE_DISK {
			return Err(StorageError::UnsafePath("only disk files are supported"));
		}
		// SAFETY: each information class writes the corresponding Windows SDK structure.
		let basic = unsafe { query::<FILE_BASIC_INFO>(file, FileBasicInfo) }?;
		let standard = unsafe { query::<FILE_STANDARD_INFO>(file, FileStandardInfo) }?;
		if basic.FileAttributes & (SPECIAL_ATTRIBUTES | FILE_ATTRIBUTE_DIRECTORY) != 0
			|| standard.Directory
			|| standard.DeletePending
			|| standard.NumberOfLinks != 1
		{
			return Err(StorageError::UnsafePath(
				"only available regular files with one hard link are supported",
			));
		}
		let len = u64::try_from(standard.EndOfFile)
			.map_err(|_| StorageError::Unsupported("the filesystem returned a negative size"))?;
		Ok(Self {
			identity: Identity::read(file)?,
			len,
			attributes: basic.FileAttributes,
			created: basic.CreationTime,
			modified: basic.LastWriteTime,
			changed: basic.ChangeTime,
		})
	}
}

/// Reads a bounded local Windows snapshot using disk file IDs and handle-relative opens.
///
/// The path must resolve through ordinary directories on a local drive. Reparse points, offline
/// attributes and alternate streams are refused. The read denies new write/delete opens while
/// the file handle lives, then reopens the path to confirm its identity and change stamp.
///
/// # Errors
///
/// Returns an error for unsupported path forms, excess bytes, a changed file, or external I/O
/// failures.
pub fn read_snapshot(path: &Path, max_bytes: usize) -> Result<FileSnapshot, StorageError> {
	let name = terminal_name(path)?;
	let path = absolute_path(path)?;
	let scope = DirectoryScope::open(&path)?;
	let file = scope.open_file(name)?;
	let before = Observation::read(&file)?;
	let size = usize::try_from(before.len).map_err(|_| StorageError::TooLarge { max_bytes })?;
	if size > max_bytes {
		return Err(StorageError::TooLarge { max_bytes });
	}
	let mut bytes = Vec::new();
	bytes.try_reserve_exact(size).map_err(|error| {
		StorageError::io(
			"reserve snapshot buffer",
			io::Error::new(io::ErrorKind::OutOfMemory, error),
		)
	})?;
	let limit = u64::try_from(max_bytes)
		.unwrap_or(u64::MAX)
		.saturating_add(1);
	(&file)
		.take(limit)
		.read_to_end(&mut bytes)
		.map_err(|error| StorageError::io("read Windows file", error))?;
	if bytes.len() > max_bytes {
		return Err(StorageError::TooLarge { max_bytes });
	}
	if bytes.len() != size || Observation::read(&file)? != before {
		return Err(StorageError::Conflict);
	}

	let reopened = DirectoryScope::open(&path).map_err(|_| StorageError::Conflict)?;
	let named = reopened
		.open_file(name)
		.map_err(|_| StorageError::Conflict)?;
	if reopened.identity != scope.identity || Observation::read(&named)? != before {
		return Err(StorageError::Conflict);
	}

	Ok(FileSnapshot {
		fingerprint: Fingerprint {
			windows: ObservedPath {
				file: before,
				directory: scope.identity,
				name: name.to_os_string(),
			},
			sha256: Sha256::digest(&bytes).into(),
		},
		bytes,
	})
}

struct DirectoryScope {
	parent: File,
	// Keep each parent open without delete sharing until the second name/identity check.
	_ancestors: Vec<File>,
	identity: Identity,
}

impl DirectoryScope {
	fn open(path: &Path) -> Result<Self, StorageError> {
		let parent = path
			.parent()
			.ok_or(StorageError::UnsafePath("missing parent directory"))?;
		Self::open_parent(parent)
	}

	fn open_parent(parent: &Path) -> Result<Self, StorageError> {
		let mut components = parent.components();
		let Some(Component::Prefix(prefix)) = components.next() else {
			return Err(StorageError::UnsafePath("a local drive letter is required"));
		};
		let drive = match prefix.kind() {
			Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
			_ => {
				return Err(StorageError::UnsafePath(
					"network and device paths are unsupported",
				));
			}
		};
		if components.next() != Some(Component::RootDir) {
			return Err(StorageError::UnsafePath(
				"a rooted local drive path is required",
			));
		}
		let root = format!("{}:\\", char::from(drive));
		require_local_drive(&root)?;
		let mut current = OpenOptions::new()
			.access_mode(DIRECTORY_ACCESS)
			.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
			.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | NO_REPARSE_OR_RECALL)
			.open(root)
			.map_err(|error| StorageError::io("open drive root", error))?;
		check_directory(&current)?;
		require_volume_root(&current)?;
		let mut ancestors = Vec::new();
		for component in components {
			match component {
				Component::CurDir => {}
				Component::Normal(name) => {
					check_component(name)?;
					let mut options = RelativeOpenOptions::new();
					options
						.access_mode(DIRECTORY_ACCESS)
						.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
						.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | NO_REPARSE_OR_RECALL);
					let next = cap_primitives::fs::open(&current, Path::new(name), &options)
						.map_err(|error| StorageError::io("open local parent directory", error))?;
					check_directory(&next)?;
					ancestors.push(current);
					current = next;
				}
				_ => return Err(StorageError::UnsafePath("parent traversal is unsupported")),
			}
		}
		let identity = Identity::read(&current)?;
		Ok(Self {
			parent: current,
			_ancestors: ancestors,
			identity,
		})
	}

	fn open_file(&self, name: &OsStr) -> Result<File, StorageError> {
		let mut options = RelativeOpenOptions::new();
		options
			.read(true)
			.share_mode(FILE_SHARE_READ)
			.custom_flags(NO_REPARSE_OR_RECALL);
		cap_primitives::fs::open(&self.parent, Path::new(name), &options)
			.map_err(|error| StorageError::io("open regular Windows file", error))
	}
}

pub(crate) fn backup_entries(
	path: &Path,
) -> Result<impl Iterator<Item = Result<PathBuf, StorageError>> + '_, StorageError> {
	let scope = DirectoryScope::open_parent(path)?;
	let entries = cap_primitives::fs::read_base_dir(&scope.parent)
		.map_err(|error| StorageError::io("read Windows backup directory", error))?;
	Ok(entries.map(move |entry| {
		// Keep the directory and its ancestors open without delete sharing.
		let _scope = &scope;
		entry
			.map(|entry| path.join(entry.file_name()))
			.map_err(|error| StorageError::io("read Windows backup entry", error))
	}))
}

fn absolute_path(path: &Path) -> Result<PathBuf, StorageError> {
	if path
		.components()
		.any(|component| component == Component::ParentDir)
	{
		return Err(StorageError::UnsafePath("parent traversal is unsupported"));
	}
	if path.is_absolute() {
		return Ok(path.to_path_buf());
	}
	if path.has_root() || matches!(path.components().next(), Some(Component::Prefix(_))) {
		return Err(StorageError::UnsafePath("ambiguous drive-relative path"));
	}
	std::env::current_dir()
		.map(|current| current.join(path))
		.map_err(|error| StorageError::io("resolve current directory", error))
}

fn terminal_name(path: &Path) -> Result<&OsStr, StorageError> {
	let name = path
		.file_name()
		.ok_or(StorageError::UnsafePath("missing regular file name"))?;
	let leaf = path
		.as_os_str()
		.as_encoded_bytes()
		.rsplit(|byte| matches!(byte, b'/' | b'\\'))
		.next();
	if leaf != Some(name.as_encoded_bytes()) {
		return Err(StorageError::UnsafePath("path must end with the file name"));
	}
	check_component(name)?;
	Ok(name)
}

fn check_component(name: &OsStr) -> Result<(), StorageError> {
	let name = name.as_encoded_bytes();
	if name
		.iter()
		.any(|byte| matches!(byte, 0 | b':' | b'/' | b'\\'))
		|| name.last().is_some_and(|byte| matches!(byte, b'.' | b' '))
	{
		return Err(StorageError::UnsafePath(
			"ambiguous path component or alternate stream",
		));
	}
	Ok(())
}

fn require_local_drive(root: &str) -> Result<(), StorageError> {
	let root: Vec<u16> = OsStr::new(root).encode_wide().chain([0]).collect();
	// SAFETY: root is NUL-terminated and stays allocated through the Win32 call.
	let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
	if !matches!(
		kind,
		DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_CDROM | DRIVE_RAMDISK
	) {
		return Err(StorageError::Unsupported("a local drive is required"));
	}
	Ok(())
}

// Checking the opened root as well as its drive letter rules out redirection of the drive
// mapping between resolution and open. Network shares never receive local volume GUID paths.
fn require_volume_root(root: &File) -> Result<(), StorageError> {
	let mut value = [0; LOCAL_ROOT_GUID_CAPACITY as usize];
	// SAFETY: the directory handle is live and value has space for the declared UTF-16 units.
	let length = unsafe {
		GetFinalPathNameByHandleW(
			root.as_raw_handle(),
			value.as_mut_ptr(),
			LOCAL_ROOT_GUID_CAPACITY,
			VOLUME_NAME_GUID,
		)
	};
	if length == 0 || length >= LOCAL_ROOT_GUID_CAPACITY {
		return Err(StorageError::Unsupported(
			"the opened drive is not a local volume root",
		));
	}
	let path = String::from_utf16_lossy(&value[..length as usize]);
	if !path.starts_with(r"\\?\Volume{") || !path.ends_with("}\\") {
		return Err(StorageError::Unsupported(
			"the opened drive is not a local volume root",
		));
	}
	Ok(())
}

fn check_directory(directory: &File) -> Result<(), StorageError> {
	let metadata = directory
		.metadata()
		.map_err(|error| StorageError::io("stat directory", error))?;
	if !metadata.is_dir() || metadata.file_attributes() & SPECIAL_ATTRIBUTES != 0 {
		return Err(StorageError::UnsafePath(
			"the parent is not an ordinary local directory",
		));
	}
	Ok(())
}

// SAFETY: callers must choose the writable Windows SDK structure corresponding to class and
// its layout must remain valid for any values written by the system.
unsafe fn query<T: Default>(
	file: &File,
	class: FILE_INFO_BY_HANDLE_CLASS,
) -> Result<T, StorageError> {
	let mut value = T::default();
	let size = u32::try_from(size_of::<T>()).map_err(|error| {
		StorageError::io("size Windows file information", io::Error::other(error))
	})?;
	// SAFETY: the borrow stays live and the caller identifies the buffer type for the class.
	let result = unsafe {
		GetFileInformationByHandleEx(file.as_raw_handle(), class, (&raw mut value).cast(), size)
	};
	if result == 0 {
		return Err(StorageError::io(
			"query Windows file information",
			io::Error::last_os_error(),
		));
	}
	Ok(value)
}

#[cfg(test)]
mod rename_qualification;
