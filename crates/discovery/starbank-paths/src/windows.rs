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
	path::PathBuf,
	ptr::{null, null_mut},
	slice,
};

use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
use windows_sys::Win32::{
	Foundation::{E_FAIL, RPC_E_CHANGED_MODE},
	Globalization::lstrlenW,
	Storage::FileSystem::{
		FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_READ_ATTRIBUTES, FileIdInfo,
		GetFileInformationByHandleEx,
	},
	System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
	UI::Shell::{FOLDERID_Documents, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath},
};

use crate::PathError;

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

pub(crate) fn file_identity(dir: &Dir, name: &OsStr) -> io::Result<(u64, [u8; 16])> {
	let mut options = OpenOptions::new();
	options
		.read(true)
		.access_mode(FILE_READ_ATTRIBUTES)
		.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
	let file = dir.open_with(name, &options)?;
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
	Ok((data.VolumeSerialNumber, data.FileId.Identifier))
}
