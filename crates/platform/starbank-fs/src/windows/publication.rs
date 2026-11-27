//! Handle-relative no-replace publication using the qualified NT rename request layout.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::marker::PhantomData;
use std::mem::{align_of, offset_of, size_of};
use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};

use crate::StorageError;
use windows_sys::{
	Wdk::Storage::FileSystem::{
		FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation,
		NtSetInformationFile,
	},
	Win32::{
		Foundation::{HANDLE, STATUS_OBJECT_NAME_COLLISION, STATUS_PENDING, STATUS_SUCCESS},
		System::IO::IO_STATUS_BLOCK,
	},
};

const MAX_NAME_UNITS: usize = 255;

#[repr(C)]
pub(super) struct RenameRequest<'parent> {
	flags: FILE_RENAME_INFORMATION_0,
	parent: HANDLE,
	name_bytes: u32,
	name: [u16; MAX_NAME_UNITS],
	_lifetime: PhantomData<&'parent File>,
}

const _: () = {
	assert!(align_of::<RenameRequest>() == align_of::<FILE_RENAME_INFORMATION>());
	assert!(size_of::<RenameRequest>() >= size_of::<FILE_RENAME_INFORMATION>());
	assert!(offset_of!(RenameRequest, flags) == offset_of!(FILE_RENAME_INFORMATION, Anonymous));
	assert!(
		offset_of!(RenameRequest, parent) == offset_of!(FILE_RENAME_INFORMATION, RootDirectory)
	);
	assert!(
		offset_of!(RenameRequest, name_bytes)
			== offset_of!(FILE_RENAME_INFORMATION, FileNameLength)
	);
	assert!(offset_of!(RenameRequest, name) == offset_of!(FILE_RENAME_INFORMATION, FileName));
};

#[derive(Debug)]
pub(super) enum Publication {
	Published,
	Collision,
	Indeterminate(StorageError),
}

impl<'parent> RenameRequest<'parent> {
	pub(super) fn new(parent: &'parent File, name: &OsStr) -> Result<Self, StorageError> {
		super::check_component(name)?;
		if name.is_empty() {
			return Err(StorageError::UnsafePath("empty rename leaf"));
		}
		let mut request = Self {
			flags: FILE_RENAME_INFORMATION_0 { Flags: 0 },
			parent: parent.as_raw_handle(),
			name_bytes: 0,
			name: [0; MAX_NAME_UNITS],
			_lifetime: PhantomData,
		};
		let mut units = 0;
		for (index, unit) in name.encode_wide().enumerate() {
			*request
				.name
				.get_mut(index)
				.ok_or(StorageError::UnsafePath("rename leaf exceeds NTFS limit"))? = unit;
			units += 1;
		}
		request.name_bytes = u32::try_from(units * size_of::<u16>())
			.map_err(|error| StorageError::io("size rename leaf", io::Error::other(error)))?;
		Ok(request)
	}

	fn byte_size(&self) -> u32 {
		// Construction bounds the name to 255 UTF-16 units; both sizes fit u32 on Windows.
		(offset_of!(Self, name) + self.name_bytes as usize)
			.max(size_of::<FILE_RENAME_INFORMATION>()) as u32
	}

	pub(super) fn publish(&self, file: &File) -> Publication {
		let mut completion = IO_STATUS_BLOCK::default();
		// SAFETY: caller retains the synchronous nonalert stage and parent handles for this
		// request. The aligned SDK-layout buffer has an initialized counted UTF-16 tail.
		// Zero rename flags prohibit replacing any destination. Direct completed status is
		// authoritative. See the FILE_RENAME_INFORMATION and IO_STATUS_BLOCK contracts.
		let status = unsafe {
			NtSetInformationFile(
				file.as_raw_handle(),
				&mut completion,
				(self as *const Self).cast(),
				self.byte_size(),
				FileRenameInformation,
			)
		};
		match status {
			STATUS_SUCCESS => Publication::Published,
			STATUS_OBJECT_NAME_COLLISION => Publication::Collision,
			STATUS_PENDING => {
				// A broken synchronous-handle invariant must not free a potentially live IOSB.
				std::process::abort();
			}
			_ => Publication::Indeterminate(StorageError::io(
				"publish pending file",
				io::Error::other(format!("NtSetInformationFile NTSTATUS {status:#010x}")),
			)),
		}
	}
}
