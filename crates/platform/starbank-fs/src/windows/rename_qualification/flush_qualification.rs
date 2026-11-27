//! Observed flush capability only, not namespace durability or power survival.
//!
//! https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers
//! https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/nf-ntifs-zwflushbuffersfileex

use super::super::{DIRECTORY_ACCESS, NO_REPARSE_OR_RECALL};
use super::{
	DESTINATION_NAME, DirectoryScope, EXISTING, MAX_BYTES, Observation, PreparedStage, STAGED,
	STAGED_NAME, assert_absent, read_snapshot, verify_published,
};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::mem::size_of;
use std::os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle};
use std::path::Path;
use std::ptr;
use windows_sys::{
	Wdk::Storage::FileSystem::{
		FILE_ACCESS_INFORMATION, FILE_MODE_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT,
		FileAccessInformation, FileModeInformation, NtFlushBuffersFileEx, NtQueryInformationFile,
	},
	Win32::{
		Foundation::{
			ERROR_ACCESS_DENIED, GENERIC_READ, GENERIC_WRITE, STATUS_ACCESS_DENIED,
			STATUS_OBJECT_NAME_COLLISION, STATUS_PENDING, STATUS_SUCCESS,
		},
		Storage::FileSystem::{
			DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
			FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
			FlushFileBuffers,
		},
		System::IO::{IO_STATUS_BLOCK, IO_STATUS_BLOCK_0},
	},
};

const FLUSH_DIRECTORY: &str = "FlushCapability";
const WRITABLE_DIRECTORY_ACCESS: u32 = GENERIC_WRITE | FILE_READ_ATTRIBUTES | FILE_TRAVERSE;
const DIRECTORY_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;
const DIRECTORY_FLAGS: u32 = FILE_FLAG_BACKUP_SEMANTICS | NO_REPARSE_OR_RECALL;
// Normal native flush includes data, metadata and underlying storage synchronization.
const NORMAL_NATIVE_FLUSH: u32 = 0;
#[derive(Clone, Copy, Debug)]
enum HandleKind {
	File,
	Directory,
}
#[derive(Clone, Copy, Debug)]
enum FlushCapability {
	Succeeded,
	AccessUnavailable,
}

pub(super) fn qualify_flush_capabilities(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
	let path = root.join(FLUSH_DIRECTORY);
	fs::create_dir(&path).map_err(|error| phase_error("create flush fixture directory", error))?;
	let scope = DirectoryScope::open_parent(&path)?;
	super::require_ntfs(&scope.parent)?;
	eprintln!(
		"Flush token context: preceding security probe records actual elevation; no privileges enabled or volume handles opened here. No namespace-durability or power-survival claim."
	);
	eprintln!(
		"Traversal directory requested access={DIRECTORY_ACCESS:#010x}, flags={DIRECTORY_FLAGS:#010x}, share={DIRECTORY_SHARE:#010x}"
	);
	let granted = describe_handle("traversal directory", &scope.parent)?;
	assert_eq!(granted & DIRECTORY_ACCESS, DIRECTORY_ACCESS);
	let writable = open_writable_directory(&path, &scope)?;
	let stage =
		PreparedStage::create(&scope).map_err(|error| phase_error("create flush stage", error))?;
	let original = Observation::read(&stage.file)?;
	eprintln!(
		"PreparedStage::create already completed sync_all; subsequent file measurements are additional capability calls, not independent writeback experiments"
	);
	let requested_file = GENERIC_READ | GENERIC_WRITE | DELETE;
	eprintln!(
		"Staged file requested access={requested_file:#010x}, flags={NO_REPARSE_OR_RECALL:#010x}, share={FILE_SHARE_READ:#010x}; cap-primitives uses synchronous nonalert mode"
	);
	let granted = describe_handle("staged file", &stage.file)?;
	let required = FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE;
	assert_eq!(granted & required, required);
	measure_flush("file before rename", &stage.file, HandleKind::File)?;
	measure_directories("before rename", &scope.parent, writable.as_ref())?;
	let destination = path.join(DESTINATION_NAME);
	// Verify that the already-flushed prepared handle still refuses an existing destination.
	fs::write(&destination, EXISTING)?;
	let before = read_snapshot(&destination, MAX_BYTES)?;
	let status = stage.publish_status(&scope, OsStr::new(DESTINATION_NAME))?;
	assert_eq!(
		status, STATUS_OBJECT_NAME_COLLISION,
		"flush collision NTSTATUS {status:#010x}"
	);
	let after = read_snapshot(&destination, MAX_BYTES)?;
	assert_eq!(after.fingerprint(), before.fingerprint());
	assert_eq!(after.bytes(), EXISTING);
	assert_ne!(after.fingerprint().windows.file.identity, original.identity);
	assert_eq!(Observation::read(&stage.file)?.identity, original.identity);
	assert_eq!(fs::read(path.join(STAGED_NAME))?, STAGED);
	fs::remove_file(&destination)?;
	let status = stage.publish_status(&scope, OsStr::new(DESTINATION_NAME))?;
	assert_eq!(
		status, STATUS_SUCCESS,
		"flush publication NTSTATUS {status:#010x}"
	);
	measure_flush("file after rename", &stage.file, HandleKind::File)?;
	measure_directories("after rename", &scope.parent, writable.as_ref())?;
	assert_eq!(Observation::read(&stage.file)?.identity, original.identity);
	drop(stage);
	verify_published(&path, &scope, original.identity)?;
	assert_absent(&path.join(STAGED_NAME));
	eprintln!(
		"Flush observations complete; identity/digest/name binding and collision preserved. Call success does not establish namespace persistence."
	);
	Ok(())
}

fn measure_directories(phase: &str, parent: &File, writable: Option<&File>) -> io::Result<()> {
	measure_flush(
		&format!("traversal directory {phase}"),
		parent,
		HandleKind::Directory,
	)?;
	if let Some(parent) = writable {
		measure_flush(
			&format!("writable directory {phase}"),
			parent,
			HandleKind::Directory,
		)?;
	}
	Ok(())
}

fn open_writable_directory(path: &Path, expected: &DirectoryScope) -> io::Result<Option<File>> {
	let mut options = OpenOptions::new();
	options
		.access_mode(WRITABLE_DIRECTORY_ACCESS)
		.share_mode(DIRECTORY_SHARE)
		.custom_flags(DIRECTORY_FLAGS);
	eprintln!(
		"Exploratory writable directory CreateFileW requested access={WRITABLE_DIRECTORY_ACCESS:#010x}, flags={DIRECTORY_FLAGS:#010x}, share={DIRECTORY_SHARE:#010x}"
	);
	// Hold the directory and ancestors without delete sharing, then verify the reopened
	// object matches that scope before flushing (including drive mapping changes).
	let parent = match options.open(path) {
		Ok(parent) => parent,
		Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
			eprintln!(
				"Writable directory open AccessUnavailable: Win32 error {ERROR_ACCESS_DENIED}; no writable-directory flush claim"
			);
			return Ok(None);
		}
		Err(error) => return Err(phase_error("open writable directory", error)),
	};
	assert!(parent.metadata()?.is_dir());
	assert_eq!(
		super::Identity::read(&parent).map_err(io::Error::other)?,
		expected.identity
	);
	let granted = describe_handle("writable directory", &parent)?;
	let required = FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES | FILE_TRAVERSE;
	assert_eq!(granted & required, required);
	Ok(Some(parent))
}

fn measure_flush(label: &str, file: &File, kind: HandleKind) -> io::Result<()> {
	// SAFETY: the live handle is a synchronous file/directory opened by this fixture.
	let win32_result = unsafe { FlushFileBuffers(file.as_raw_handle()) };
	// Capture last-error before logging or issuing another API call.
	let win32_error = if win32_result == 0 {
		Some(io::Error::last_os_error())
	} else {
		None
	};
	let win32_capability = match win32_error {
		None => FlushCapability::Succeeded,
		Some(error)
			if matches!(kind, HandleKind::Directory)
				&& error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) =>
		{
			FlushCapability::AccessUnavailable
		}
		Some(error) => return Err(phase_error(label, error)),
	};
	eprintln!(
		"{label} FlushFileBuffers {win32_capability:?}, raw error={:?}",
		if matches!(win32_capability, FlushCapability::AccessUnavailable) {
			Some(ERROR_ACCESS_DENIED)
		} else {
			None
		}
	);
	let mut completion = IO_STATUS_BLOCK {
		Anonymous: IO_STATUS_BLOCK_0 {
			Status: STATUS_PENDING,
		},
		Information: 0,
	};
	// SAFETY: live synchronous handles (checked by FileModeInformation) and writable IOSB.
	// Flag zero selects normal flush; this call supplies no optional parameter block.
	let status = unsafe {
		NtFlushBuffersFileEx(
			file.as_raw_handle(),
			NORMAL_NATIVE_FLUSH,
			ptr::null(),
			0,
			&mut completion,
		)
	};
	require_completed(label, status);
	// SAFETY: Status was initialized. Rejected calls can leave it untouched; direct
	// synchronous status, not this diagnostic field, determines the outcome.
	let completion_status = unsafe { completion.Anonymous.Status };
	let native_capability = match status {
		STATUS_SUCCESS => FlushCapability::Succeeded,
		STATUS_ACCESS_DENIED if matches!(kind, HandleKind::Directory) => {
			FlushCapability::AccessUnavailable
		}
		_ => {
			eprintln!(
				"{label} NtFlushBuffersFileEx unexpected NTSTATUS {status:#010x}, output status={completion_status:#010x}"
			);
			return Err(io::Error::other(format!(
				"{label} native flush NTSTATUS {status:#010x}"
			)));
		}
	};
	eprintln!(
		"{label} NtFlushBuffersFileEx flags={NORMAL_NATIVE_FLUSH:#010x} {native_capability:?}, NTSTATUS={status:#010x}, output status={completion_status:#010x}, information={}",
		completion.Information
	);
	Ok(())
}

fn describe_handle(label: &str, file: &File) -> io::Result<u32> {
	let mut access = FILE_ACCESS_INFORMATION::default();
	let mut mode = FILE_MODE_INFORMATION::default();
	let mut completion = IO_STATUS_BLOCK::default();
	// SAFETY: SDK structures and matching information classes, fixed writable sizes,
	// and live synchronous handles; outputs are inspected only after direct success.
	let access_status = unsafe {
		NtQueryInformationFile(
			file.as_raw_handle(),
			&mut completion,
			(&raw mut access).cast(),
			size_of::<FILE_ACCESS_INFORMATION>() as u32,
			FileAccessInformation,
		)
	};
	require_completed(label, access_status);
	if access_status != STATUS_SUCCESS {
		return Err(io::Error::other(format!(
			"{label} query access NTSTATUS {access_status:#010x}"
		)));
	}
	assert_eq!(completion.Information, size_of::<FILE_ACCESS_INFORMATION>());
	// SAFETY: same fixed-size SDK/query-class contract for file mode information.
	let mode_status = unsafe {
		NtQueryInformationFile(
			file.as_raw_handle(),
			&mut completion,
			(&raw mut mode).cast(),
			size_of::<FILE_MODE_INFORMATION>() as u32,
			FileModeInformation,
		)
	};
	require_completed(label, mode_status);
	if mode_status != STATUS_SUCCESS {
		return Err(io::Error::other(format!(
			"{label} query mode NTSTATUS {mode_status:#010x}"
		)));
	}
	assert_eq!(completion.Information, size_of::<FILE_MODE_INFORMATION>());
	assert_ne!(
		mode.Mode & FILE_SYNCHRONOUS_IO_NONALERT,
		0,
		"fixture requires synchronous handle"
	);
	eprintln!(
		"{label} observed access={:#010x}, mode-options-subset={:#010x}, query NTSTATUS={access_status:#010x}/{mode_status:#010x}",
		access.AccessFlags, mode.Mode
	);
	Ok(access.AccessFlags)
}

fn phase_error(label: &str, error: io::Error) -> io::Error {
	eprintln!("Flush capability phase '{label}' failed: {error:?}");
	error
}

fn require_completed(label: &str, status: i32) {
	if status == STATUS_PENDING {
		eprintln!(
			"{label}: synchronous native request unexpectedly pending; aborting without releasing its stack-backed I/O status block"
		);
		// A broken synchronous-handle assumption must not free a potentially live IOSB.
		std::process::abort();
	}
}
