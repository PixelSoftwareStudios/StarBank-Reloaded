//! Prepare an empty, test-owned temporary directory for the reduced actor.

use std::{
	error::Error,
	fs::{self, OpenOptions},
	io,
	os::windows::{
		fs::{MetadataExt as _, OpenOptionsExt as _},
		io::AsRawHandle as _,
	},
	path::PathBuf,
	ptr,
};

use windows_sys::Win32::{
	Foundation::ERROR_SUCCESS,
	Security::{
		Authorization::{SE_FILE_OBJECT, SetSecurityInfo},
		CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
		OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
	},
	Storage::FileSystem::{
		FILE_ALL_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
		FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
		WRITE_DAC,
	},
};

use super::super::require_ntfs;
use super::{SecurityObservation, descriptor, restricted_token::source_user, win32};
use crate::windows::{DirectoryScope, Identity, NO_REPARSE_OR_RECALL, check_directory};

pub(in crate::windows) fn prepare_reduced_fixture(
	fixture: &tempfile::TempDir,
) -> Result<PathBuf, Box<dyn Error>> {
	let metadata = fs::symlink_metadata(fixture.path())?;
	if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
		return Err(io::Error::other("expected an ordinary NTFS test directory").into());
	}
	let path = fixture.path().canonicalize()?;
	let scope = DirectoryScope::open_parent(&path)?;
	require_ntfs(&scope.parent)?;
	let acl_edit = OpenOptions::new()
		.access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | READ_CONTROL | WRITE_DAC)
		.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
		.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | NO_REPARSE_OR_RECALL)
		.open(&path)?;
	check_directory(&acl_edit)?;
	if Identity::read(&acl_edit)? != scope.identity {
		return Err(io::Error::other("the NTFS test fixture directory changed").into());
	}
	if cap_primitives::fs::read_base_dir(&acl_edit)?
		.next()
		.transpose()?
		.is_some()
	{
		return Err(
			io::Error::other("the NTFS test fixture must be empty before ACL setup").into(),
		);
	}
	let user = source_user()?;
	let security = descriptor(&format!("D:P(A;OICI;FA;;;{user})"))?;
	let (mut present, mut dacl, mut defaulted) = (0, ptr::null_mut(), 0);
	// SAFETY: the security allocation and all three writable result slots are live.
	win32(unsafe {
		GetSecurityDescriptorDacl(security.0, &mut present, &mut dacl, &mut defaulted)
	})?;
	if present == 0 || dacl.is_null() {
		return Err(io::Error::other("the fixture ACL must be present and non-null").into());
	}
	// SAFETY: the handle is the verified empty temporary directory with WRITE_DAC. The DACL
	// points into the still-live security allocation. Only the fixture directory DACL is set.
	let result = unsafe {
		SetSecurityInfo(
			acl_edit.as_raw_handle(),
			SE_FILE_OBJECT,
			DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
			ptr::null_mut(),
			ptr::null_mut(),
			dacl,
			ptr::null(),
		)
	};
	if result != ERROR_SUCCESS {
		return Err(io::Error::from_raw_os_error(result.cast_signed()).into());
	}
	let observed = SecurityObservation::read(&acl_edit)?;
	assert_ne!(observed.control & SE_DACL_PROTECTED, 0);
	assert!(
		matches!(observed.dacl.as_slice(), [ace] if ace.sid == user
		&& ace.mask == FILE_ALL_ACCESS
		&& u32::from(ace.flags) == OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE),
		"empty NTFS fixture must grant explicit inherited access to TokenUser"
	);
	Ok(path)
}
