//! Synthetic kernel qualification of read-back while a restrictive stage handle remains live.

use std::{
	error::Error,
	ffi::OsStr,
	fs::{self, File},
	io::{self, Read as _},
	path::Path,
};

use cap_primitives::fs::{OpenOptions, OpenOptionsExt as _};
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::{
	Foundation::{ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, STATUS_SUCCESS},
	Storage::FileSystem::{
		DELETE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
	},
};

use super::{
	DESTINATION_NAME, DirectoryScope, NO_REPARSE_OR_RECALL, Observation, PreparedStage, STAGED,
	STAGED_NAME, assert_absent, require_ntfs, verify_published,
};

const FIXTURE_DIRECTORY: &str = "HeldStageReadback";
const READ_BACK_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

pub(super) fn qualify_held_stage_readback(root: &Path) -> Result<(), Box<dyn Error>> {
	let folder = root.join(FIXTURE_DIRECTORY);
	fs::create_dir(&folder)?;
	let scope = DirectoryScope::open_parent(&folder)?;
	require_ntfs(&scope.parent)?;
	let leaf = OsStr::new(DESTINATION_NAME);

	// PreparedStage deliberately inherits fixture security here. Creation-time privacy is
	// qualified separately. Both verification and competing opens use this runner's identity.
	let stage = PreparedStage::create(&scope)?;
	let staged = Observation::read(&stage.file)?;
	assert_eq!(staged.identity.volume, scope.identity.volume);
	assert_eq!(staged.len, u64::try_from(STAGED.len())?);

	let status = stage.publish_status(&scope, leaf)?;
	assert_eq!(
		status, STATUS_SUCCESS,
		"held-stage rename NTSTATUS {status:#010x}"
	);
	assert_absent(&folder.join(STAGED_NAME));

	let published = Observation::read(&stage.file)?;
	assert_eq!(published.identity, staged.identity);
	assert_eq!(published.len, staged.len);
	let verifier = open_scoped(&scope, leaf, GENERIC_READ | READ_CONTROL, READ_BACK_SHARE)?;
	assert_eq!(Observation::read(&verifier)?, published);
	eprintln!(
		"Held-stage read-back opened an independent handle with identity {:?} and length {}",
		published.identity, published.len
	);

	// The ordinary reader does not share the existing stage's write/delete access.
	// Conversely, permissive share flags on a new handle cannot grant write/delete access.
	for (label, access, share) in [
		("ordinary read sharing", GENERIC_READ, FILE_SHARE_READ),
		("competing write access", GENERIC_WRITE, READ_BACK_SHARE),
		("competing delete access", DELETE, READ_BACK_SHARE),
	] {
		let error = open_scoped(&scope, leaf, access, share)
			.expect_err("incompatible open must fail while the stage handle lives");
		let code = error.raw_os_error();
		eprintln!("{label} while holding stage returned Win32 status {code:?}");
		assert_eq!(
			code,
			Some(ERROR_SHARING_VIOLATION.cast_signed()),
			"{label}: {error}"
		);
	}

	let mut read = &verifier;
	let mut bytes = [0; STAGED.len()];
	read.read_exact(&mut bytes)?;
	assert_eq!(read.read(&mut [0])?, 0, "no extra bytes past the fixture");
	let digest = Sha256::digest(bytes);
	assert_eq!(digest, Sha256::digest(STAGED));
	assert_eq!(bytes, STAGED);
	assert_eq!(Observation::read(&verifier)?, published);
	assert_eq!(Observation::read(&stage.file)?, published);
	eprintln!(
		"Held-stage read-back verified SHA-256 {}",
		hex::encode(digest)
	);

	drop(verifier);
	drop(stage);

	// Control for ACL or access-mode mistakes: obtaining each access now succeeds. Merely
	// opening a handle with these rights does not write bytes or mark the file for deletion.
	for access in [GENERIC_WRITE, DELETE] {
		drop(open_scoped(&scope, leaf, access, READ_BACK_SHARE)?);
	}
	verify_published(&folder, &scope, staged.identity)?;
	assert_absent(&folder.join(STAGED_NAME));
	Ok(())
}

fn open_scoped(scope: &DirectoryScope, leaf: &OsStr, access: u32, share: u32) -> io::Result<File> {
	let mut options = OpenOptions::new();
	options
		.access_mode(access)
		.share_mode(share)
		.custom_flags(NO_REPARSE_OR_RECALL);
	cap_primitives::fs::open(&scope.parent, Path::new(leaf), &options)
}
