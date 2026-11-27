//! Bound local NTFS destination and readback compatible with the retained writer handle.

use super::super::{
	DirectoryScope, Identity, NO_REPARSE_OR_RECALL, Observation, ObservedPath, absolute_path,
	check_directory, terminal_name,
};
use crate::{FileSnapshot, Fingerprint, StorageError};
use cap_primitives::fs::{OpenOptions as RelativeOptions, OpenOptionsExt as _};
use sha2::{Digest as _, Sha256};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Path, PathBuf};
use std::ptr;
use windows_sys::Win32::{
	Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
	Storage::FileSystem::{
		FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
		FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, FileCaseSensitiveInfo,
		GetVolumeInformationByHandleW, READ_CONTROL, ReOpenFile,
	},
};

const FS_NAME_UNITS: u32 = 32;
const WRITABLE_PARENT_ACCESS: u32 = GENERIC_WRITE | FILE_READ_ATTRIBUTES | FILE_TRAVERSE;

pub(super) struct Scope {
	pub(super) directory: DirectoryScope,
	pub(super) writable: File,
	pub(super) path: PathBuf,
	pub(super) name: OsString,
}

impl Scope {
	pub(super) fn open(path: &Path) -> Result<Self, StorageError> {
		let name = terminal_name(path)?.to_os_string();
		super::leaf::validate(&name)?;
		let path = absolute_path(path)?;
		let directory = DirectoryScope::open(&path)?;
		require_ntfs(&directory.parent)?;
		require_case_insensitive(&directory.parent)?;
		// Reopen the held object instead of resolving another writable path. Qualify the
		// actual cap-primitives handle; API refusal fails before staging, with no fallback.
		// https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-reopenfile
		// SAFETY: original parent stays live; directory flags and no-delete sharing are
		// documented for ReOpenFile. No ambient writable-path traversal is introduced.
		let writable = unsafe {
			ReOpenFile(
				directory.parent.as_raw_handle(),
				WRITABLE_PARENT_ACCESS,
				FILE_SHARE_READ | FILE_SHARE_WRITE,
				FILE_FLAG_BACKUP_SEMANTICS | NO_REPARSE_OR_RECALL,
			)
		};
		if writable == INVALID_HANDLE_VALUE {
			return Err(StorageError::io(
				"reopen writable destination directory",
				io::Error::last_os_error(),
			));
		}
		// SAFETY: ReOpenFile transferred ownership of the successful new handle.
		let writable = unsafe { File::from_raw_handle(writable) };
		check_directory(&writable)?;
		if Identity::read(&writable)? != directory.identity {
			return Err(StorageError::Conflict);
		}
		let scope = Self {
			directory,
			writable,
			path,
			name,
		};
		scope.ensure_bound()?;
		Ok(scope)
	}

	pub(super) fn ensure_bound(&self) -> Result<(), StorageError> {
		let fresh = DirectoryScope::open(&self.path)?;
		if fresh.identity != self.directory.identity
			|| Identity::read(&self.writable)? != self.directory.identity
		{
			return Err(StorageError::Conflict);
		}
		require_case_insensitive(&self.directory.parent)?;
		check_directory(&self.writable)
	}

	pub(super) fn open_file(&self, name: &OsStr) -> Result<File, StorageError> {
		super::super::check_component(name)?;
		let mut options = RelativeOptions::new();
		options
			.access_mode(GENERIC_READ | READ_CONTROL)
			.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
			.custom_flags(NO_REPARSE_OR_RECALL);
		cap_primitives::fs::open(&self.directory.parent, Path::new(name), &options)
			.map_err(|error| StorageError::io("open Windows commit readback", error))
	}

	pub(super) fn absent(&self, name: &OsStr) -> Result<(), StorageError> {
		match self.open_file(name) {
			Err(StorageError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
				Ok(())
			}
			Ok(_) => Err(StorageError::Conflict),
			Err(error) => Err(error),
		}
	}

	pub(super) fn snapshot(
		&self,
		name: &OsStr,
		max_bytes: usize,
	) -> Result<(File, FileSnapshot), StorageError> {
		self.read_file(name, self.open_file(name)?, max_bytes)
	}

	pub(super) fn strict_snapshot(
		&self,
		name: &OsStr,
		max_bytes: usize,
	) -> Result<(File, FileSnapshot), StorageError> {
		self.read_file(name, self.directory.open_file(name)?, max_bytes)
	}

	fn read_file(
		&self,
		name: &OsStr,
		file: File,
		max_bytes: usize,
	) -> Result<(File, FileSnapshot), StorageError> {
		let before = Observation::read(&file)?;
		let size = usize::try_from(before.len).map_err(|_| StorageError::TooLarge { max_bytes })?;
		if size > max_bytes {
			return Err(StorageError::TooLarge { max_bytes });
		}
		let mut bytes = Vec::new();
		bytes.try_reserve_exact(size).map_err(|error| {
			StorageError::io(
				"reserve commit readback",
				io::Error::new(io::ErrorKind::OutOfMemory, error),
			)
		})?;
		let limit = u64::try_from(max_bytes)
			.unwrap_or(u64::MAX)
			.saturating_add(1);
		(&file)
			.take(limit)
			.read_to_end(&mut bytes)
			.map_err(|error| StorageError::io("read Windows commit", error))?;
		if bytes.len() > max_bytes {
			return Err(StorageError::TooLarge { max_bytes });
		}
		if bytes.len() != size || Observation::read(&file)? != before {
			return Err(StorageError::Conflict);
		}
		let fingerprint = Fingerprint {
			windows: ObservedPath {
				file: before,
				directory: self.directory.identity,
				name: name.to_os_string(),
			},
			sha256: Sha256::digest(&bytes).into(),
		};
		Ok((file, FileSnapshot { fingerprint, bytes }))
	}
}

fn require_ntfs(directory: &File) -> Result<(), StorageError> {
	let mut name = [0; FS_NAME_UNITS as usize];
	// SAFETY: held directory handle and correctly sized writable filesystem-name buffer.
	let result = unsafe {
		GetVolumeInformationByHandleW(
			directory.as_raw_handle(),
			ptr::null_mut(),
			0,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			name.as_mut_ptr(),
			FS_NAME_UNITS,
		)
	};
	if result == 0 {
		return Err(StorageError::io(
			"query destination filesystem",
			io::Error::last_os_error(),
		));
	}
	let length = name
		.iter()
		.position(|unit| *unit == 0)
		.ok_or(StorageError::Unsupported("unterminated filesystem name"))?;
	if !name[..length].iter().copied().eq("NTFS".encode_utf16()) {
		return Err(StorageError::Unsupported(
			"Windows creation requires local NTFS",
		));
	}
	Ok(())
}

fn require_case_insensitive(directory: &File) -> Result<(), StorageError> {
	// SAFETY: SDK structure exactly matches this handle information class. Unknown query
	// support or flags fail closed; native alias qualification covers flags zero only.
	let case = unsafe {
		super::super::query::<FILE_CASE_SENSITIVE_INFO>(directory, FileCaseSensitiveInfo)
	}?;
	if case.Flags != 0 {
		return Err(StorageError::Unsupported(
			"Windows creation requires a case-insensitive directory",
		));
	}
	Ok(())
}
