use std::{
	error::Error,
	ffi::OsStr,
	fmt,
	fs::File,
	io,
	mem::size_of,
	os::windows::{
		ffi::OsStrExt as _,
		io::{AsRawHandle as _, FromRawHandle as _},
	},
	ptr,
};

#[cfg(test)]
use windows_sys::{
	Wdk::Storage::FileSystem::FILE_DIRECTORY_FILE,
	Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_SHARE_WRITE},
};
use windows_sys::{
	Wdk::{
		Foundation::OBJECT_ATTRIBUTES,
		Storage::FileSystem::{
			FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT,
			FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
		},
	},
	Win32::{
		Foundation::{
			HANDLE, NTSTATUS, STATUS_OBJECT_NAME_COLLISION, STATUS_SUCCESS, UNICODE_STRING,
		},
		Storage::FileSystem::{FILE_ATTRIBUTE_ARCHIVE, FILE_SHARE_READ},
		System::{IO::IO_STATUS_BLOCK, WindowsProgramming::FILE_CREATED},
	},
};

use crate::StorageError;

use super::{super::check_component, SecurityDescriptor};

const MAX_NTFS_LEAF_UNITS: usize = 255;

pub(in crate::windows) enum RelativeKind {
	File,
	#[cfg(test)]
	Directory,
}

pub(super) fn create_private_file(
	parent: &File,
	leaf: &OsStr,
	access: u32,
	descriptor: &SecurityDescriptor,
) -> Result<File, StorageError> {
	create_relative_inner(parent, leaf, access, RelativeKind::File, Some(descriptor))
}

#[cfg(test)]
pub(in crate::windows) fn create_relative(
	parent: &File,
	leaf: &OsStr,
	access: u32,
	kind: RelativeKind,
	descriptor: Option<&SecurityDescriptor>,
) -> Result<File, StorageError> {
	create_relative_inner(parent, leaf, access, kind, descriptor)
}

fn create_relative_inner(
	parent: &File,
	leaf: &OsStr,
	access: u32,
	kind: RelativeKind,
	descriptor: Option<&SecurityDescriptor>,
) -> Result<File, StorageError> {
	let mut units = checked_leaf(leaf)?;
	let length = u16::try_from(units.len() * size_of::<u16>())
		.map_err(|_| StorageError::UnsafePath("Windows leaf byte length exceeds its limit"))?;
	let mut name = UNICODE_STRING {
		Length: length,
		MaximumLength: length,
		Buffer: units.as_mut_ptr(),
	};
	let object = OBJECT_ATTRIBUTES {
		Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
		RootDirectory: parent.as_raw_handle(),
		ObjectName: &mut name,
		Attributes: 0,
		SecurityDescriptor: descriptor.map_or(ptr::null(), |descriptor| {
			descriptor.as_ptr().cast_const().cast()
		}),
		SecurityQualityOfService: ptr::null_mut(),
	};
	let (share, attributes, flags) = match kind {
		RelativeKind::File => (
			FILE_SHARE_READ,
			FILE_ATTRIBUTE_ARCHIVE,
			FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_OPEN_NO_RECALL,
		),
		#[cfg(test)]
		RelativeKind::Directory => (
			FILE_SHARE_READ | FILE_SHARE_WRITE,
			FILE_ATTRIBUTE_NORMAL,
			FILE_DIRECTORY_FILE,
		),
	};
	let options = flags | FILE_SYNCHRONOUS_IO_NONALERT;
	let mut handle: HANDLE = ptr::null_mut();
	let mut completion = IO_STATUS_BLOCK::default();
	// SAFETY: the parent and optional descriptor stay live, the checked counted leaf and
	// SDK structures are initialized, and synchronous mode completes the call before any
	// storage leaves scope. FILE_CREATE refuses an existing entry.
	let status = unsafe {
		NtCreateFile(
			&mut handle,
			access,
			&object,
			&mut completion,
			ptr::null(),
			attributes,
			share,
			FILE_CREATE,
			options,
			ptr::null_mut(),
			0,
		)
	};
	if status != STATUS_SUCCESS {
		let kind = if status == STATUS_OBJECT_NAME_COLLISION {
			io::ErrorKind::AlreadyExists
		} else {
			io::ErrorKind::Other
		};
		return Err(StorageError::io(
			"create Windows stage relative to held parent",
			io::Error::new(kind, CreateFailure { status, options }),
		));
	}
	if handle.is_null() {
		return Err(StorageError::UnsafeMetadata(
			"Windows stage has no file handle",
		));
	}
	// SAFETY: a successful native call transfers this one owned handle.
	let file = unsafe { File::from_raw_handle(handle) };
	if completion.Information != FILE_CREATED as usize {
		return Err(StorageError::UnsafeMetadata(
			"Windows stage was not newly created",
		));
	}
	Ok(file)
}

fn checked_leaf(leaf: &OsStr) -> Result<Vec<u16>, StorageError> {
	check_component(leaf)?;
	let units: Vec<_> = leaf.encode_wide().take(MAX_NTFS_LEAF_UNITS + 1).collect();
	if units.is_empty() || units.len() > MAX_NTFS_LEAF_UNITS {
		return Err(StorageError::UnsafePath(
			"Windows stage requires a bounded file name",
		));
	}
	Ok(units)
}

#[derive(Debug)]
struct CreateFailure {
	status: NTSTATUS,
	options: u32,
}

impl fmt::Display for CreateFailure {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			formatter,
			"NtCreateFile options {:#010x}, NTSTATUS {:#010x}",
			self.options, self.status
		)
	}
}

impl Error for CreateFailure {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn checks_exclusive_create_leaf_in_native_units_before_opening() {
		let astral = "𐐀".repeat(MAX_NTFS_LEAF_UNITS / 2);
		let exact = format!("{astral}a");
		assert_eq!(
			checked_leaf(OsStr::new(&exact)).unwrap().len(),
			MAX_NTFS_LEAF_UNITS,
		);
		let over = format!("{exact}b");
		for refused in ["", "file:stream", "nested\\file", "..", &over] {
			assert!(matches!(
				checked_leaf(OsStr::new(refused)),
				Err(StorageError::UnsafePath(_)),
			));
		}
	}
}
