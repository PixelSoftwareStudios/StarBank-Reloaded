//! Darwin file metadata and publication capabilities.
#![allow(unsafe_code)]

use std::ffi::{OsString, c_int, c_void};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::ptr::{self, NonNull};

use rustix::fs::{self, RenameFlags};

use crate::StorageError;

const APFS_NAME: &[u8] = b"apfs\0";
const ACL_TYPE_EXTENDED: c_int = 0x0000_0100;
const MAX_SECURITY_BYTES: usize = 16 * 1024 * 1024;
const RENAME_NOFOLLOW_ANY: u32 = 0x0000_0010;

unsafe extern "C" {
	fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
	fn acl_size(acl: *mut c_void) -> isize;
	fn acl_copy_ext(buffer: *mut c_void, acl: *mut c_void, size: isize) -> isize;
	fn acl_free(object: *mut c_void) -> c_int;
}

#[derive(PartialEq, Eq)]
pub(super) struct Security {
	acl: Option<Vec<u8>>,
	xattrs: Vec<(OsString, Vec<u8>)>,
}

pub(super) fn require_local_apfs(dir: &File) -> Result<(), StorageError> {
	let filesystem =
		fs::fstatfs(dir).map_err(|error| StorageError::io("inspect filesystem", error))?;
	let local = filesystem.f_flags & (libc::MNT_LOCAL as u32) != 0;
	let filesystem_name = filesystem
		.f_fstypename
		.iter()
		.map(|byte| byte.to_ne_bytes()[0]);
	if !local
		|| !filesystem_name
			.take(APFS_NAME.len())
			.eq(APFS_NAME.iter().copied())
	{
		return Err(StorageError::Unsupported(
			"verified macOS commits require local APFS",
		));
	}
	Ok(())
}

pub(super) fn publication_flags(flags: RenameFlags) -> RenameFlags {
	flags | RenameFlags::from_bits_retain(RENAME_NOFOLLOW_ANY)
}

pub(super) fn require_writable_metadata(file: &File) -> Result<(), StorageError> {
	let stat = fs::fstat(file).map_err(|error| StorageError::io("inspect file flags", error))?;
	let mode = fs::Mode::from_raw_mode(stat.st_mode);
	if stat.st_flags != 0 || mode.intersects(fs::Mode::SUID | fs::Mode::SGID | fs::Mode::SVTX) {
		return Err(StorageError::UnsafeMetadata(
			"file flags and special permission bits are unsupported for overwrite",
		));
	}
	if !mode.intersects(fs::Mode::WUSR | fs::Mode::WGRP | fs::Mode::WOTH) {
		return Err(StorageError::UnsafeMetadata("file is read-only"));
	}
	Ok(())
}

pub(super) fn ensure_private_stage(file: &File) -> Result<(), StorageError> {
	if read_acl(file)?.is_some() {
		return Err(StorageError::UnsafeMetadata(
			"pending file inherited an ACL",
		));
	}
	Ok(())
}

pub(super) fn metadata(file: &File) -> Result<Security, StorageError> {
	require_writable_metadata(file)?;
	Ok(Security {
		acl: read_acl(file)?,
		xattrs: read_xattrs(file)?,
	})
}

pub(super) fn copy_metadata(source: &File, replacement: &File) -> Result<(), StorageError> {
	// SAFETY: Both descriptors stay open during the call. Null selects temporary native
	// state owned by fcopyfile. The metadata flags exclude the main data stream.
	let result = unsafe {
		libc::fcopyfile(
			source.as_raw_fd(),
			replacement.as_raw_fd(),
			ptr::null_mut(),
			libc::COPYFILE_METADATA,
		)
	};
	if result != 0 {
		return Err(StorageError::io(
			"copy file metadata",
			io::Error::last_os_error(),
		));
	}
	Ok(())
}

fn read_acl(file: &File) -> Result<Option<Vec<u8>>, StorageError> {
	// SAFETY: The raw file descriptor is borrowed from a live File. Darwin returns an
	// owned acl_t or null, with ENOENT indicating the file has no extended ACL.
	let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
	let Some(acl) = NonNull::new(acl).map(Acl) else {
		let error = io::Error::last_os_error();
		return if error.kind() == io::ErrorKind::NotFound {
			Ok(None)
		} else {
			Err(StorageError::io("read file ACL", error))
		};
	};
	// SAFETY: acl is a live acl_t whose allocation is owned by the guard.
	let bytes = unsafe { acl_size(acl.0.as_ptr()) };
	let size = usize::try_from(bytes)
		.map_err(|_| StorageError::io("measure file ACL", io::Error::last_os_error()))?;
	if size > MAX_SECURITY_BYTES {
		return Err(StorageError::UnsafeMetadata(
			"ACL exceeds the verification limit",
		));
	}
	let mut encoded = vec![0; size];
	// SAFETY: encoded has `bytes` initialized bytes and remains allocated for the call.
	// acl is a live read-only input guard.
	let copied = unsafe { acl_copy_ext(encoded.as_mut_ptr().cast(), acl.0.as_ptr(), bytes) };
	if copied != bytes {
		return Err(StorageError::UnsafeMetadata(
			"could not verify the ACL encoding",
		));
	}
	Ok(Some(encoded))
}

struct Acl(NonNull<c_void>);

impl Drop for Acl {
	fn drop(&mut self) {
		// SAFETY: This guard is the unique owner of a live acl_t returned by Darwin.
		unsafe { acl_free(self.0.as_ptr()) };
	}
}

fn read_xattrs(file: &File) -> Result<Vec<(OsString, Vec<u8>)>, StorageError> {
	let size = fs::flistxattr_query(file)
		.map_err(|error| StorageError::io("measure file extended attributes", error))?;
	let mut remaining =
		MAX_SECURITY_BYTES
			.checked_sub(size)
			.ok_or(StorageError::UnsafeMetadata(
				"extended attributes exceed the verification limit",
			))?;
	if size == 0 {
		return Ok(Vec::new());
	}
	let mut list = vec![0; size];
	let read = fs::flistxattr(file, list.as_mut_slice())
		.map_err(|error| StorageError::io("list file extended attributes", error))?;
	if read != size || list.last() != Some(&0) {
		return Err(StorageError::Conflict);
	}
	let mut attributes = Vec::new();
	for name in list[..read - 1].split(|byte| *byte == 0) {
		if name.is_empty() {
			return Err(StorageError::UnsafeMetadata(
				"invalid extended attribute name",
			));
		}
		let name = OsString::from_vec(name.to_vec());
		let size = fs::fgetxattr_query(file, &name)
			.map_err(|error| StorageError::io("measure extended attribute", error))?;
		remaining = remaining
			.checked_sub(size)
			.ok_or(StorageError::UnsafeMetadata(
				"extended attributes exceed the verification limit",
			))?;
		let mut data = vec![0; size];
		if size != 0 {
			let read = fs::fgetxattr(file, &name, data.as_mut_slice())
				.map_err(|error| StorageError::io("read extended attribute", error))?;
			if read != size {
				return Err(StorageError::Conflict);
			}
		}
		attributes.push((name, data));
	}
	attributes.sort_unstable_by(|first, second| first.0.cmp(&second.0));
	Ok(attributes)
}
