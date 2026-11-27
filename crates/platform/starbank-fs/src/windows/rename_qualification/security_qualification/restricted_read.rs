//! Read-access proof on the live synthetic stage using the restricted same-user reader.

use std::{
	ffi::OsStr,
	fs::File,
	io::{self, Read},
	path::Path,
};

use cap_primitives::fs::{OpenOptions, OpenOptionsExt as _};
use windows_sys::Win32::{
	Foundation::ERROR_ACCESS_DENIED,
	Storage::FileSystem::{
		FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
	},
};

use super::super::super::NO_REPARSE_OR_RECALL;
use super::{INHERITED_CONTROL, STAGED, STAGED_NAME, restricted_token::Actors};

pub(super) const CONTROL_BYTES: &[u8] = b"inherited readable test fixture";

pub(super) fn qualify_restricted_read(directory: &File, actors: &Actors) -> io::Result<()> {
	// The creator uses exactly the same open mode while the stage's writer handle is live.
	verify_read(directory, STAGED_NAME, STAGED)?;
	actors.as_reader(|_| {
		verify_read(directory, INHERITED_CONTROL, CONTROL_BYTES)?;
		let denial = open_data(directory, STAGED_NAME)
			.expect_err("the World-restricted reader must not read the private stage");
		assert_eq!(
			denial.raw_os_error(),
			Some(ERROR_ACCESS_DENIED.cast_signed())
		);
		Ok(())
	})?;
	eprintln!(
		"Same-user World-restricted read: inherited control bytes verified, private stage denied with ERROR_ACCESS_DENIED, creator read verified with identical sharing"
	);
	Ok(())
}

fn verify_read(directory: &File, leaf: &str, expected: &[u8]) -> io::Result<()> {
	let mut file = open_data(directory, leaf)?;
	let mut bytes = vec![0; expected.len()];
	file.read_exact(&mut bytes)?;
	assert_eq!(bytes, expected);
	assert_eq!(file.read(&mut [0])?, 0);
	Ok(())
}

fn open_data(directory: &File, leaf: &str) -> io::Result<File> {
	let mut options = OpenOptions::new();
	// Sharing includes the retained writer's WRITE and DELETE access. The new handle only
	// requests read data and synchronization. The backend adds read attributes. A denial
	// is distinguishable from a share violation with these matching positive controls.
	options
		.read(true)
		.access_mode(FILE_READ_DATA | SYNCHRONIZE)
		.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
		.custom_flags(NO_REPARSE_OR_RECALL);
	cap_primitives::fs::open(directory, Path::new(OsStr::new(leaf)), &options)
}
