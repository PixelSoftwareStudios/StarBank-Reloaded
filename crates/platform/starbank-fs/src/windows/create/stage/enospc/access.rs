//! Test-only FILE_OPEN probes after the real writable scope has failed.

use crate::windows::{
	DIRECTORY_ACCESS, DirectoryScope, Identity, check_component, check_directory,
	stage_security::diagnostics::{report_descriptor, report_token},
};
use std::{
	error::Error,
	ffi::OsStr,
	fs::File,
	io,
	mem::size_of,
	os::windows::{
		ffi::OsStrExt as _,
		io::{AsRawHandle as _, FromRawHandle as _},
	},
	path::Path,
	ptr,
};
use windows_sys::{
	Wdk::{
		Foundation::OBJECT_ATTRIBUTES,
		Storage::FileSystem::{
			FILE_OPEN, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
			NtCreateFile,
		},
	},
	Win32::{
		Foundation::{INVALID_HANDLE_VALUE, STATUS_PENDING, STATUS_SUCCESS, UNICODE_STRING},
		Security::{SECURITY_QUALITY_OF_SERVICE, SECURITY_STATIC_TRACKING, SecurityAnonymous},
		Storage::FileSystem::{
			FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE,
			FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
			FILE_WRITE_ATTRIBUTES, FILE_WRITE_EA, READ_CONTROL,
		},
		System::{IO::IO_STATUS_BLOCK, WindowsProgramming::FILE_OPENED},
	},
};

const CAP_ACCESS: u32 = FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES | FILE_TRAVERSE;
const READ_BASE: u32 = DIRECTORY_ACCESS | READ_CONTROL;
const OPTIONS: u32 = FILE_OPEN_NO_RECALL | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT;
const SHARING: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;

enum QualityOfService {
	MatchedCap,
	Absent,
}

pub(super) fn report(path: &Path, owned: &DirectoryScope) -> Result<(), Box<dyn Error>> {
	// Reopen under the same reduced actor as the failure, with the same read-only scope.
	let scoped = DirectoryScope::open_parent(path)?;
	if scoped.identity != owned.identity {
		return Err(io::Error::other("access probe fixture identity changed").into());
	}
	let Some(ancestor) = scoped._ancestors.last() else {
		return Err(io::Error::other("access probe requires a verified ancestor").into());
	};
	let leaf = path
		.file_name()
		.ok_or_else(|| io::Error::other("access probe requires an ordinary leaf"))?;
	check_component(leaf)?;

	report_token();
	for (name, qos) in [
		("full matched cap", QualityOfService::MatchedCap),
		("full without QoS", QualityOfService::Absent),
	] {
		drop(probe(
			name,
			ancestor,
			leaf,
			scoped.identity,
			CAP_ACCESS,
			qos,
		)?);
	}
	drop(probe(
		"read base",
		ancestor,
		leaf,
		scoped.identity,
		DIRECTORY_ACCESS,
		QualityOfService::MatchedCap,
	)?);
	if let Some(read) = probe(
		"read plus READ_CONTROL",
		ancestor,
		leaf,
		scoped.identity,
		READ_BASE,
		QualityOfService::MatchedCap,
	)? {
		report_descriptor(&read);
	}
	for (name, right) in [
		("add file", FILE_ADD_FILE),
		("add subdirectory", FILE_ADD_SUBDIRECTORY),
		("write extended attributes", FILE_WRITE_EA),
		("write attributes", FILE_WRITE_ATTRIBUTES),
	] {
		drop(probe(
			name,
			ancestor,
			leaf,
			scoped.identity,
			DIRECTORY_ACCESS | right,
			QualityOfService::MatchedCap,
		)?);
	}
	Ok(())
}

fn probe(
	name: &str,
	parent: &File,
	leaf: &OsStr,
	expected: Identity,
	access: u32,
	qos: QualityOfService,
) -> Result<Option<File>, Box<dyn Error>> {
	let mut units: Vec<_> = leaf.encode_wide().collect();
	let byte_len = u16::try_from(units.len() * size_of::<u16>())?;
	let mut native_leaf = UNICODE_STRING {
		Length: byte_len,
		MaximumLength: byte_len,
		Buffer: units.as_mut_ptr(),
	};
	// cap-primitives 4.0.3 also treats OPEN_NO_RECALL as SECURITY_SQOS_PRESENT.
	let mut native_qos = SECURITY_QUALITY_OF_SERVICE {
		Length: size_of::<SECURITY_QUALITY_OF_SERVICE>() as u32,
		ImpersonationLevel: SecurityAnonymous,
		ContextTrackingMode: u8::from(SECURITY_STATIC_TRACKING),
		EffectiveOnly: false,
	};
	let qos_matches_cap = matches!(qos, QualityOfService::MatchedCap);
	let attributes = OBJECT_ATTRIBUTES {
		Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
		RootDirectory: parent.as_raw_handle(),
		ObjectName: &mut native_leaf,
		Attributes: 0,
		SecurityDescriptor: ptr::null(),
		SecurityQualityOfService: if qos_matches_cap {
			(&raw mut native_qos).cast()
		} else {
			ptr::null()
		},
	};
	let mut handle = ptr::null_mut();
	let mut completion = IO_STATUS_BLOCK::default();
	completion.Anonymous.Status = STATUS_PENDING;
	// SAFETY: all SDK records and output slots stay live, the counted leaf is relative to
	// the held verified ancestor, and FILE_OPEN with synchronous flags only opens an existing object.
	let status = unsafe {
		NtCreateFile(
			&mut handle,
			access,
			&attributes,
			&mut completion,
			ptr::null(),
			FILE_ATTRIBUTE_NORMAL,
			SHARING,
			FILE_OPEN,
			OPTIONS,
			ptr::null(),
			0,
		)
	};
	if status == STATUS_PENDING {
		eprintln!("NTFS access probe: synchronous open unexpectedly pending");
		// A pending result must not release the stack-backed IOSB.
		std::process::abort();
	}
	// SAFETY: initialized to STATUS_PENDING; NtCreateFile may replace it with a final status.
	let io_status = unsafe { completion.Anonymous.Status };
	eprintln!(
		"NTFS access probe {name}: access={access:#010x} options={OPTIONS:#010x} 		 share={SHARING:#04x} anonymous_qos={qos_matches_cap} status={status:#010x} 		 io_status={io_status:#010x}"
	);
	if status != STATUS_SUCCESS {
		return Ok(None);
	}
	if handle.is_null() || handle == INVALID_HANDLE_VALUE {
		return Err(io::Error::other("successful NTFS access probe returned no handle").into());
	}
	// SAFETY: successful NtCreateFile transfers an owned, live file handle.
	let file = unsafe { File::from_raw_handle(handle) };
	if completion.Information != FILE_OPENED as usize {
		return Err(io::Error::other("NTFS access probe did not report FILE_OPENED").into());
	}
	check_directory(&file)?;
	if Identity::read(&file)? != expected {
		return Err(io::Error::other("NTFS access probe opened a different directory").into());
	}
	Ok(Some(file))
}
