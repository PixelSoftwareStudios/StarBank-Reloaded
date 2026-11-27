//! Opt-in kernel probe on synthetic files. The product writer does not call this code.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::mem::{align_of, offset_of, size_of};
use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
use std::path::Path;
use std::ptr;

use cap_primitives::fs::{OpenOptions, OpenOptionsExt as _};
use sha2::{Digest as _, Sha256};
use windows_sys::{
	Wdk::Storage::FileSystem::{
		FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation,
		NtSetInformationFile,
	},
	Win32::{
		Foundation::{
			GENERIC_READ, GENERIC_WRITE, HANDLE, NTSTATUS, STATUS_OBJECT_NAME_COLLISION,
			STATUS_SUCCESS,
		},
		Storage::FileSystem::{DELETE, FILE_SHARE_READ, GetVolumeInformationByHandleW},
		System::IO::IO_STATUS_BLOCK,
	},
};

use super::{DirectoryScope, Identity, NO_REPARSE_OR_RECALL, Observation, read_snapshot};

mod readback_qualification;
mod security_qualification;

const STAGED_NAME: &str = "PendingBank.sc2bank";
const DESTINATION_NAME: &str = "PublishedBank.SC2Bank";
const CASE_ALIAS: &str = "pUBLISHEDbANK.sc2bank";
const CONTROL_SOURCE_DIR: &str = "RootControlSource";
const CONTROL_TARGET_DIR: &str = "RootControlTarget";
const STAGED: &[u8] = b"<Bank><!-- prepared fixture --></Bank>";
const EXISTING: &[u8] = b"<Bank><!-- competing fixture --></Bank>";
const MAX_BYTES: usize = 1024;
const MAX_NAME_UNITS: usize = 255;
const FS_NAME_CAPACITY: u32 = 32;

#[repr(C)]
struct RenameBuffer {
	flags: FILE_RENAME_INFORMATION_0,
	parent: HANDLE,
	name_bytes: u32,
	name: [u16; MAX_NAME_UNITS],
}

// Compare the flexible-tail buffer with the SDK header at compile time. The name is inline at
// the FileName offset, including on targets where the SDK header has trailing alignment padding.
const _: () = {
	assert!(align_of::<RenameBuffer>() == align_of::<FILE_RENAME_INFORMATION>());
	assert!(size_of::<RenameBuffer>() >= size_of::<FILE_RENAME_INFORMATION>());
	assert!(offset_of!(RenameBuffer, flags) == offset_of!(FILE_RENAME_INFORMATION, Anonymous));
	assert!(offset_of!(RenameBuffer, parent) == offset_of!(FILE_RENAME_INFORMATION, RootDirectory));
	assert!(
		offset_of!(RenameBuffer, name_bytes) == offset_of!(FILE_RENAME_INFORMATION, FileNameLength)
	);
	assert!(offset_of!(RenameBuffer, name) == offset_of!(FILE_RENAME_INFORMATION, FileName));
};

impl RenameBuffer {
	fn new(parent: HANDLE, name: &OsStr) -> io::Result<Self> {
		super::check_component(name).map_err(io::Error::other)?;
		if name.is_empty() {
			return Err(io::Error::new(
				io::ErrorKind::InvalidInput,
				"empty rename leaf",
			));
		}
		let mut buffer = Self {
			flags: FILE_RENAME_INFORMATION_0 { Flags: 0 },
			parent,
			name_bytes: 0,
			name: [0; MAX_NAME_UNITS],
		};
		let mut name_units = 0;
		for (index, unit) in name.encode_wide().enumerate() {
			let slot = buffer.name.get_mut(index).ok_or_else(|| {
				io::Error::new(
					io::ErrorKind::InvalidInput,
					"rename leaf exceeds NTFS name limit",
				)
			})?;
			*slot = unit;
			name_units += 1;
		}
		buffer.name_bytes =
			u32::try_from(name_units * size_of::<u16>()).map_err(io::Error::other)?;
		Ok(buffer)
	}

	fn byte_size(&self) -> io::Result<u32> {
		let name_bytes = usize::try_from(self.name_bytes).map_err(io::Error::other)?;
		let length = offset_of!(Self, name) + name_bytes;
		u32::try_from(length.max(size_of::<FILE_RENAME_INFORMATION>())).map_err(io::Error::other)
	}
}

struct PreparedStage<'parent> {
	parent: &'parent DirectoryScope,
	file: File,
}

impl<'parent> PreparedStage<'parent> {
	fn create(parent: &'parent DirectoryScope) -> io::Result<Self> {
		let mut options = OpenOptions::new();
		options
			.write(true)
			.access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
			.share_mode(FILE_SHARE_READ)
			.custom_flags(NO_REPARSE_OR_RECALL)
			.create_new(true);
		let mut file = cap_primitives::fs::open(&parent.parent, Path::new(STAGED_NAME), &options)?;
		file.write_all(STAGED)?;
		file.sync_all()?;
		Ok(Self { parent, file })
	}

	fn publish_status(&self, destination: &DirectoryScope, name: &OsStr) -> io::Result<NTSTATUS> {
		assert_eq!(self.parent.identity.volume, destination.identity.volume);
		let buffer = RenameBuffer::new(destination.parent.as_raw_handle(), name)?;
		let length = buffer.byte_size()?;
		let mut completion = IO_STATUS_BLOCK::default();
		// SAFETY: the private stage is created without overlapped I/O. cap-primitives opens
		// it with FILE_SYNCHRONOUS_IO_NONALERT and SYNCHRONIZE, so the request completes in
		// the call. The live stage and directory handles, initialized writable status block
		// and aligned counted UTF-16 buffer outlive the call. Zero flags disable replacement.
		// For completed requests, the direct NTSTATUS is authoritative over the status block.
		Ok(unsafe {
			NtSetInformationFile(
				self.file.as_raw_handle(),
				&mut completion,
				(&raw const buffer).cast(),
				length,
				FileRenameInformation,
			)
		})
	}
}

#[test]
#[ignore = "requires a native Windows host with its temporary directory on local NTFS"]
fn local_ntfs_handle_relative_create_only_publication() -> Result<(), Box<dyn std::error::Error>> {
	let temporary = tempfile::TempDir::new()?;
	let root = temporary.path().canonicalize()?;
	let scope = DirectoryScope::open_parent(&root)?;
	require_ntfs(&scope.parent)?;
	eprintln!("NTFS probe parent identity {:?}", scope.identity);
	verify_explicit_destination_root(&root)?;

	let staged = root.join(STAGED_NAME);
	let destination = root.join(DESTINATION_NAME);
	let stage = PreparedStage::create(&scope)?;
	let original_stage = Observation::read(&stage.file)?;
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

		let status = stage.publish_status(&scope, OsStr::new(DESTINATION_NAME))?;
		eprintln!(
			"Exclusive collision for {spelling} returned NTSTATUS {status:#010x}; occupant {occupant:?}"
		);
		assert_eq!(
			status, STATUS_OBJECT_NAME_COLLISION,
			"NTSTATUS {status:#010x}"
		);
		let after = read_snapshot(&competing, MAX_BYTES)?;
		assert_eq!(after.fingerprint(), before.fingerprint());
		assert_eq!(after.bytes(), EXISTING);
		assert_eq!(
			Observation::read(&stage.file)?.identity,
			original_stage.identity
		);
		assert_eq!(fs::read(&staged)?, STAGED);
		fs::remove_file(competing)?;
	}

	let status = stage.publish_status(&scope, OsStr::new(DESTINATION_NAME))?;
	eprintln!("Exclusive publication returned NTSTATUS {status:#010x}");
	assert_eq!(status, STATUS_SUCCESS, "NTSTATUS {status:#010x}");
	assert_eq!(Identity::read(&stage.file)?, original_stage.identity);
	drop(stage);

	verify_published(&root, &scope, original_stage.identity)?;
	assert_absent(&staged);
	readback_qualification::qualify_held_stage_readback(&root)?;
	security_qualification::qualify_private_stage(&root)?;
	Ok(())
}

fn verify_explicit_destination_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
	let source = root.join(CONTROL_SOURCE_DIR);
	let target = root.join(CONTROL_TARGET_DIR);
	fs::create_dir(&source)?;
	fs::create_dir(&target)?;
	let source_scope = DirectoryScope::open_parent(&source)?;
	let target_scope = DirectoryScope::open_parent(&target)?;
	assert_eq!(source_scope.identity.volume, target_scope.identity.volume);
	assert_ne!(source_scope.identity, target_scope.identity);
	let source_leaf = source.join(DESTINATION_NAME);
	assert_absent(&source_leaf);
	assert_absent(&target.join(DESTINATION_NAME));

	let stage = PreparedStage::create(&source_scope)?;
	let original_stage = Observation::read(&stage.file)?;
	assert_eq!(original_stage.identity.volume, source_scope.identity.volume);
	let status = stage.publish_status(&target_scope, OsStr::new(DESTINATION_NAME))?;
	eprintln!(
		"Explicit destination root {:?}, distinct source root {:?}, NTSTATUS {status:#010x}",
		target_scope.identity, source_scope.identity
	);
	assert_eq!(status, STATUS_SUCCESS, "NTSTATUS {status:#010x}");
	assert_eq!(Identity::read(&stage.file)?, original_stage.identity);
	drop(stage);

	verify_published(&target, &target_scope, original_stage.identity)?;
	assert_absent(&source.join(STAGED_NAME));
	assert_absent(&source_leaf);
	Ok(())
}

fn assert_absent(path: &Path) {
	assert!(
		fs::symlink_metadata(path).is_err_and(|error| error.kind() == io::ErrorKind::NotFound),
		"expected absent fixture {path:?}"
	);
}

fn verify_published(
	parent: &Path,
	scope: &DirectoryScope,
	stage_identity: Identity,
) -> Result<(), Box<dyn std::error::Error>> {
	let resolved = DirectoryScope::open_parent(parent)?;
	assert_eq!(resolved.identity, scope.identity);
	let published = read_snapshot(&parent.join(DESTINATION_NAME), MAX_BYTES)?;
	assert_eq!(
		published.fingerprint().windows.file.identity,
		stage_identity
	);
	assert_eq!(published.fingerprint().windows.directory, scope.identity);
	assert_eq!(published.bytes(), STAGED);
	let digest: [u8; 32] = Sha256::digest(STAGED).into();
	assert_eq!(published.fingerprint().sha256(), &digest);
	eprintln!(
		"Verified stage identity {stage_identity:?} at the destination, size {}, SHA-256 {}",
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

#[test]
fn rename_buffer_counts_utf16_bytes_without_a_terminator() -> Result<(), Box<dyn std::error::Error>>
{
	let short = RenameBuffer::new(ptr::null_mut(), OsStr::new("x"))?;
	assert_eq!(short.name_bytes, 2);
	assert_eq!(
		usize::try_from(short.byte_size()?)?,
		size_of::<FILE_RENAME_INFORMATION>()
	);

	let multibyte = "é🚀.SC2Bank";
	let buffer = RenameBuffer::new(ptr::null_mut(), OsStr::new(multibyte))?;
	assert_eq!(buffer.name_bytes, 22);
	assert!(
		buffer.name[..11]
			.iter()
			.copied()
			.eq(multibyte.encode_utf16())
	);
	assert_eq!(
		usize::try_from(buffer.byte_size()?)?,
		offset_of!(RenameBuffer, name) + 22
	);

	let longest = "x".repeat(MAX_NAME_UNITS);
	let buffer = RenameBuffer::new(ptr::null_mut(), OsStr::new(&longest))?;
	assert_eq!(
		usize::try_from(buffer.byte_size()?)?,
		offset_of!(RenameBuffer, name) + size_of_val(&buffer.name)
	);
	Ok(())
}

#[test]
fn rename_buffer_refuses_ambiguous_and_out_of_bounds_names() {
	for leaf in ["", "..", "a/b", r"a\b", "a:b", "a\0b", "a.", "a "] {
		assert!(RenameBuffer::new(ptr::null_mut(), OsStr::new(leaf)).is_err());
	}
	let too_long = "x".repeat(MAX_NAME_UNITS + 1);
	assert!(RenameBuffer::new(ptr::null_mut(), OsStr::new(&too_long)).is_err());
}
