//! Narrow Win32 calls for user Documents and metadata-only file identity.
#![allow(
	unsafe_code,
	reason = "Win32 known-folder and file-identity FFI is isolated here"
)]

use std::{
	ffi::{OsStr, OsString},
	io,
	mem::{MaybeUninit, size_of},
	os::windows::{ffi::OsStringExt, io::AsRawHandle},
	path::{Path, PathBuf},
	ptr::{null, null_mut},
	slice,
};

use cap_std::fs::{Dir, File, Metadata, MetadataExt, OpenOptions, OpenOptionsExt};
use windows_sys::Win32::{
	Foundation::{
		E_FAIL, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
		RPC_E_CHANGED_MODE,
	},
	Globalization::lstrlenW,
	Storage::FileSystem::{
		FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
		FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
		FILE_FLAG_OPEN_NO_RECALL, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_READ_ATTRIBUTES,
		FileIdInfo, GetFileInformationByHandleEx,
	},
	System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
	UI::Shell::{FOLDERID_Documents, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath},
};

use crate::PathError;

const LOCAL_LEAF_FLAGS: u32 = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL;

struct ComApartment(bool);

impl ComApartment {
	fn initialize() -> Result<Self, PathError> {
		// SAFETY: reserved argument is null; successful calls are balanced by Drop
		// on the same thread. A changed mode means this thread already has COM.
		let status = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED.cast_unsigned()) };
		if status >= 0 {
			Ok(Self(true))
		} else if status == RPC_E_CHANGED_MODE {
			Ok(Self(false))
		} else {
			Err(PathError::KnownFolder(status))
		}
	}
}

impl Drop for ComApartment {
	fn drop(&mut self) {
		if self.0 {
			// SAFETY: this guard owns one successful initialization on this thread.
			unsafe { CoUninitialize() };
		}
	}
}

struct KnownFolderPath(*mut u16);

impl Drop for KnownFolderPath {
	fn drop(&mut self) {
		// SAFETY: COM owns the returned allocation, and accepts a null pointer.
		// The allocation is released regardless of the lookup HRESULT.
		unsafe { CoTaskMemFree(self.0.cast()) };
	}
}

pub(crate) fn documents() -> Result<PathBuf, PathError> {
	let _apartment = ComApartment::initialize()?;
	let mut value = KnownFolderPath(null_mut());
	// SAFETY: use a static folder ID, current user token, and writable out-pointer.
	let status = unsafe {
		SHGetKnownFolderPath(
			&FOLDERID_Documents,
			KF_FLAG_DONT_VERIFY.cast_unsigned(),
			null_mut(),
			&mut value.0,
		)
	};
	if status < 0 {
		return Err(PathError::KnownFolder(status));
	}
	if value.0.is_null() {
		return Err(PathError::KnownFolder(E_FAIL));
	}
	// SAFETY: the successful API supplies a valid, NUL-terminated UTF-16 buffer
	// that remains allocated until value is dropped.
	let units = unsafe {
		let length =
			usize::try_from(lstrlenW(value.0)).map_err(|_| PathError::KnownFolder(E_FAIL))?;
		slice::from_raw_parts(value.0, length)
	};
	Ok(PathBuf::from(OsString::from_wide(units)))
}

/// A Windows volume and 128-bit file ID observed on a live handle.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WindowsFileIdentity(u64, [u8; 16]);

impl WindowsFileIdentity {
	/// Returns the volume serial number and file ID for persistence.
	pub const fn parts(self) -> (u64, [u8; 16]) {
		(self.0, self.1)
	}

	/// Observes a regular file inside the supplied scope using only attribute access.
	///
	/// Metadata and identity come from one handle. The outer option excludes
	/// nonregular entries and reparse points. An absent identity means the file
	/// can be inspected without reusing any result keyed by identity.
	///
	/// # Errors
	///
	/// Returns an I/O error if the entry cannot be inspected locally.
	pub fn observe(scope: &Dir, relative: &Path) -> io::Result<Option<(Metadata, Option<Self>)>> {
		let mut options = OpenOptions::new();
		options
			.read(true)
			.access_mode(FILE_READ_ATTRIBUTES)
			.custom_flags(LOCAL_LEAF_FLAGS | FILE_FLAG_BACKUP_SEMANTICS);
		Self::observe_opened(&scope.open_with(relative, &options)?)
	}

	/// Observes metadata and identity on the supplied open file without reading contents.
	///
	/// The outer option excludes nonregular entries and reparse points. The
	/// file system can report an absent identity for an otherwise readable
	/// file. Use a no-follow open if the leaf can be replaced.
	///
	/// # Errors
	///
	/// Returns an I/O error if the entry cannot be inspected locally.
	pub fn observe_opened(file: &File) -> io::Result<Option<(Metadata, Option<Self>)>> {
		let metadata = file.metadata()?;
		let attributes = metadata.file_attributes();
		if !metadata.is_file() || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
			return Ok(None);
		}
		let requires_recall = FILE_ATTRIBUTE_OFFLINE
			| FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
			| FILE_ATTRIBUTE_RECALL_ON_OPEN;
		if attributes & requires_recall != 0 {
			return Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"file is offline or requires recall",
			));
		}
		let identity = match identity_on_handle(file) {
			Ok(identity) => (identity.1 != [0; 16]).then_some(identity),
			Err(error) if identity_is_unsupported(&error) => None,
			Err(error) => return Err(error),
		};
		Ok(Some((metadata, identity)))
	}
}

fn identity_is_unsupported(error: &io::Error) -> bool {
	// This well-formed FileIdInfo query can be unimplemented by the file system.
	matches!(
		error.raw_os_error().map(i32::cast_unsigned),
		Some(ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED)
	)
}

/// Opens a scoped entry for reading without following or recalling its leaf.
///
/// Check it with [`WindowsFileIdentity::observe_opened`] before reading contents.
///
/// # Errors
///
/// Returns an I/O error if the entry cannot be opened inside the given scope.
pub fn open_windows_file_for_read(scope: &Dir, relative: &Path) -> io::Result<File> {
	let mut options = OpenOptions::new();
	options.read(true).custom_flags(LOCAL_LEAF_FLAGS);
	scope.open_with(relative, &options)
}

pub(crate) fn file_identity(dir: &Dir, name: &OsStr) -> io::Result<WindowsFileIdentity> {
	let mut options = OpenOptions::new();
	options
		.read(true)
		.access_mode(FILE_READ_ATTRIBUTES)
		.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
	identity_on_handle(&dir.open_with(name, &options)?)
}

fn identity_on_handle(file: &File) -> io::Result<WindowsFileIdentity> {
	let mut data = MaybeUninit::<FILE_ID_INFO>::uninit();
	let size = u32::try_from(size_of::<FILE_ID_INFO>()).map_err(io::Error::other)?;
	// SAFETY: the borrowed handle is live and the writable buffer has the
	// correct type and size for FileIdInfo.
	let result = unsafe {
		GetFileInformationByHandleEx(
			file.as_raw_handle(),
			FileIdInfo,
			data.as_mut_ptr().cast(),
			size,
		)
	};
	if result == 0 {
		return Err(io::Error::last_os_error());
	}
	// SAFETY: success fills the structure.
	let data = unsafe { data.assume_init() };
	Ok(WindowsFileIdentity(
		data.VolumeSerialNumber,
		data.FileId.Identifier,
	))
}
