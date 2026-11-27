//! Opt-in kernel probe on synthetic files. The product writer does not call this code.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::mem::{align_of, offset_of, size_of};
use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
use std::ptr;

use sha2::{Digest as _, Sha256};
use windows_sys::Win32::{
	Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, GENERIC_READ, GENERIC_WRITE, HANDLE},
	Storage::FileSystem::{
		DELETE, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
		FileRenameInfo, GetVolumeInformationByHandleW, SetFileInformationByHandle,
	},
};

use super::{DirectoryScope, Identity, NO_REPARSE_OR_RECALL, Observation, read_snapshot};

const STAGED_NAME: &str = "PendingBank.sc2bank";
const DESTINATION_NAME: &str = "PublishedBank.SC2Bank";
const CASE_ALIAS: &str = "pUBLISHEDbANK.sc2bank";
const STAGED: &[u8] = b"<Bank><!-- prepared fixture --></Bank>";
const EXISTING: &[u8] = b"<Bank><!-- competing fixture --></Bank>";
const MAX_BYTES: usize = 1024;
const MAX_NAME_UNITS: usize = 255;
const FS_NAME_CAPACITY: u32 = 32;

#[repr(C)]
struct RenameBuffer {
	flags: u32,
	parent: HANDLE,
	name_bytes: u32,
	name: [u16; MAX_NAME_UNITS + 1],
}

// Compare the flexible-tail buffer with the SDK header at compile time. The name is inline at
// the FileName offset, including on targets where the SDK header has trailing alignment padding.
const _: () = {
	assert!(align_of::<RenameBuffer>() == align_of::<FILE_RENAME_INFO>());
	assert!(offset_of!(RenameBuffer, flags) == offset_of!(FILE_RENAME_INFO, Anonymous));
	assert!(offset_of!(RenameBuffer, parent) == offset_of!(FILE_RENAME_INFO, RootDirectory));
	assert!(offset_of!(RenameBuffer, name_bytes) == offset_of!(FILE_RENAME_INFO, FileNameLength));
	assert!(offset_of!(RenameBuffer, name) == offset_of!(FILE_RENAME_INFO, FileName));
};

#[test]
#[ignore = "requires a native Windows host with its temporary directory on local NTFS"]
fn local_ntfs_handle_relative_create_only_publication() -> Result<(), Box<dyn std::error::Error>> {
	let temporary = tempfile::TempDir::new()?;
	let root = temporary.path().canonicalize()?;
	let scope = DirectoryScope::open_parent(&root)?;
	require_ntfs(&scope.parent)?;
	eprintln!("NTFS probe parent identity {:?}", scope.identity);

	let staged = root.join(STAGED_NAME);
	let destination = root.join(DESTINATION_NAME);
	let mut file = OpenOptions::new()
		.write(true)
		.access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
		.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
		.custom_flags(NO_REPARSE_OR_RECALL)
		.create_new(true)
		.open(&staged)?;
	file.write_all(STAGED)?;
	file.sync_all()?;
	let original_stage = Observation::read(&file)?;
	assert_eq!(original_stage.identity.volume, scope.identity.volume);

	// Create each competitor after preparing and flushing the stage. The fixture directory
	// must resolve both spellings to the same independently observed occupant.
	for spelling in [DESTINATION_NAME, CASE_ALIAS] {
		let competing = root.join(spelling);
		fs::write(&competing, EXISTING)?;
		let before = read_snapshot(&competing, MAX_BYTES)?;
		let alias = read_snapshot(&destination, MAX_BYTES).map_err(|error| {
			io::Error::other(format!("resolve case-insensitive fixture: {error}"))
		})?;
		let occupant = before.fingerprint().windows.file.identity;
		assert_eq!(alias.fingerprint().windows.file.identity, occupant);
		assert_ne!(occupant, original_stage.identity);
		assert_eq!(occupant.volume, scope.identity.volume);

		let collision = rename_create_only(&file, &scope, OsStr::new(DESTINATION_NAME))
			.expect_err("exclusive rename must refuse the competing target");
		let code = collision.raw_os_error();
		eprintln!(
			"Exclusive collision for {spelling} returned Win32 error {code:?}; occupant {occupant:?}"
		);
		assert!(matches!(
			code,
			Some(value) if value == ERROR_ALREADY_EXISTS as i32 || value == ERROR_FILE_EXISTS as i32
		));
		let after = read_snapshot(&competing, MAX_BYTES)?;
		assert_eq!(after.fingerprint(), before.fingerprint());
		assert_eq!(after.bytes(), EXISTING);
		assert_eq!(Observation::read(&file)?.identity, original_stage.identity);
		assert_eq!(fs::read(&staged)?, STAGED);
		fs::remove_file(competing)?;
	}

	rename_create_only(&file, &scope, OsStr::new(DESTINATION_NAME))?;
	assert_eq!(Identity::read(&file)?, original_stage.identity);
	drop(file);

	let resolved = DirectoryScope::open_parent(&root)?;
	assert_eq!(resolved.identity, scope.identity);
	let published = read_snapshot(&destination, MAX_BYTES)?;
	assert_eq!(
		published.fingerprint().windows.file.identity,
		original_stage.identity
	);
	assert_eq!(published.fingerprint().windows.directory, scope.identity);
	assert_eq!(published.bytes(), STAGED);
	let digest: [u8; 32] = Sha256::digest(STAGED).into();
	assert_eq!(published.fingerprint().sha256(), &digest);
	assert!(
		fs::symlink_metadata(&staged).is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
	);
	eprintln!(
		"Verified stage identity {:?} at the destination, size {}, SHA-256 {}",
		original_stage.identity,
		STAGED.len(),
		hex::encode(digest)
	);
	Ok(())
}

fn require_ntfs(parent: &File) -> io::Result<()> {
	let mut fs_name = [0; FS_NAME_CAPACITY as usize];
	// SAFETY: the parent descriptor remains live and fs_name has the declared writable length.
	let result = unsafe {
		GetVolumeInformationByHandleW(
			parent.as_raw_handle(),
			ptr::null_mut(),
			0,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			fs_name.as_mut_ptr(),
			FS_NAME_CAPACITY,
		)
	};
	if result == 0 {
		return Err(io::Error::last_os_error());
	}
	let length = fs_name
		.iter()
		.position(|unit| *unit == 0)
		.unwrap_or(fs_name.len());
	if !fs_name[..length].iter().copied().eq("NTFS".encode_utf16()) {
		return Err(io::Error::new(
			io::ErrorKind::Unsupported,
			"the opened fixture directory must be on native NTFS",
		));
	}
	Ok(())
}

fn rename_create_only(file: &File, scope: &DirectoryScope, name: &OsStr) -> io::Result<()> {
	let mut buffer = RenameBuffer {
		flags: 0,
		parent: scope.parent.as_raw_handle(),
		name_bytes: 0,
		name: [0; MAX_NAME_UNITS + 1],
	};
	super::check_component(name).map_err(io::Error::other)?;
	let name = name.encode_wide().collect::<Vec<_>>();
	if name.is_empty() || name.len() > MAX_NAME_UNITS || name.contains(&0) {
		return Err(io::Error::new(
			io::ErrorKind::InvalidInput,
			"invalid fixture name",
		));
	}
	buffer.name[..name.len()].copy_from_slice(&name);
	buffer.name_bytes = u32::try_from(name.len() * size_of::<u16>()).map_err(io::Error::other)?;
	let encoded_size = offset_of!(RenameBuffer, name) + (name.len() + 1) * size_of::<u16>();
	let buffer_size =
		u32::try_from(encoded_size.max(size_of::<FILE_RENAME_INFO>())).map_err(io::Error::other)?;
	// SAFETY: the file and parent handles and aligned inline UTF-16 buffer stay live during
	// the call. The first union field is zero, selecting no replacement. The buffer includes
	// an extra terminating zero while FileNameLength counts only the name.
	let result = unsafe {
		SetFileInformationByHandle(
			file.as_raw_handle(),
			FileRenameInfo,
			(&raw const buffer).cast(),
			buffer_size,
		)
	};
	if result == 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}
