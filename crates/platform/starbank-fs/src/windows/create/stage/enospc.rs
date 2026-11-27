//! Native disk-full qualification on a dedicated, capacity-bounded NTFS image.

use super::super::super::{
	DirectoryScope, Observation, read_snapshot,
	rename_qualification::{prepare_reduced_fixture, with_reduced_token},
};
use super::super::{CommitPhase, Scope, try_create_scoped};
use super::{CleanupFailure, PrivateStageSecurity, Stage, State, dispose};
use crate::{CommitOutcome, FileSnapshot, StorageError};
use std::{
	env,
	error::Error,
	ffi::{OsStr, OsString},
	io::{self, Read, Seek, Write},
	os::windows::{ffi::OsStrExt as _, io::AsRawHandle as _},
	path::{Path, PathBuf},
	ptr,
};
use windows_sys::Win32::{
	Foundation::{ERROR_ACCESS_DENIED, ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL},
	Storage::FileSystem::{
		FILE_ATTRIBUTE_ARCHIVE, GetDiskFreeSpaceExW, GetFinalPathNameByHandleW,
		GetVolumeInformationByHandleW, VOLUME_NAME_GUID,
	},
};

mod access;

const ROOT_ENV: &str = "STARBANK_ENOSPC_NTFS_ROOT";
const VOLUME_ENV: &str = "STARBANK_ENOSPC_NTFS_VOLUME";
const MARKER_ENV: &str = "STARBANK_ENOSPC_NTFS_MARKER";
const MARKER_FILE: &str = ".starbank-ntfs-enospc";
const MARKER_BYTES: usize = 32;
const LABEL_PREFIX: &str = "SBENOSPC-";
const LABEL_NONCE_BYTES: usize = 8;
const NAME_UNITS: u32 = 64;
const MIB: usize = 1024 * 1024;
const VOLUME_BYTES: usize = 128 * MIB;
const MINIMUM_VOLUME_BYTES: usize = 112 * MIB;
const HEADROOM: u64 = 8 * MIB as u64;
const MINIMUM_HEADROOM: u64 = 4 * MIB as u64;
const PAYLOAD_BYTES: usize = 16 * MIB;
const CONTROL_PREFIX_BYTES: usize = 4096;
const ORIGINAL_NAME: &str = "Original.SC2Bank";
const TARGET_NAME: &str = "New.SC2Bank";
const ORIGINAL: &[u8] = b"<Bank><!-- baseline --></Bank>";
const RETRY: &[u8] = b"<Bank><!-- verified retry --></Bank>";

#[test]
#[ignore = "requires the dedicated GitHub-hosted NTFS VHD provisioned by verification/windows-enospc.ps1"]
fn bounded_ntfs_disk_full_rejects_before_publication() -> Result<(), Box<dyn Error>> {
	let mount = Mount::verify()?;
	let owned = tempfile::tempdir_in(&mount.root)?;
	let owned_scope = DirectoryScope::open_parent(owned.path())?;
	assert_eq!(owned_scope.identity.volume, mount.scope.identity.volume);
	let prepared = prepare_reduced_fixture(&owned)?;
	// Keep recovery evidence on the disposable image until the fixture detaches.
	let _ = owned.keep();
	with_reduced_token(move || exercise_disk_full(&mount, &prepared, &owned_scope))?;
	Ok(())
}

struct Mount {
	root: PathBuf,
	volume: String,
	scope: DirectoryScope,
}

impl Mount {
	fn verify() -> Result<Self, Box<dyn Error>> {
		assert_eq!(env::var("GITHUB_ACTIONS")?, "true");
		assert_eq!(env::var("RUNNER_ENVIRONMENT")?, "github-hosted");

		let root = env::var(ROOT_ENV)?;
		let system_drive = env::var("SystemDrive")?;
		let [letter, b':', b'\\'] = root.as_bytes() else {
			return Err(io::Error::other("expected a literal volume drive root").into());
		};
		assert!(letter.is_ascii_alphabetic());
		assert!(!root[..2].eq_ignore_ascii_case(&system_drive));
		let root = PathBuf::from(root);
		let volume = env::var(VOLUME_ENV)?;
		let marker = env::var(MARKER_ENV)?;
		assert_eq!(marker.len(), MARKER_BYTES);
		assert!(marker.bytes().all(|byte| byte.is_ascii_hexdigit()));

		let scope = DirectoryScope::open_parent(&root)?;
		let label = format!("{LABEL_PREFIX}{}", &marker[..LABEL_NONCE_BYTES]);
		verify_volume(&scope, &volume, &label)?;
		let marker_file = scope.open_file(OsStr::new(MARKER_FILE))?;
		let observation = Observation::read(&marker_file)?;
		assert_eq!(observation.identity.volume, scope.identity.volume);
		assert_eq!(observation.len, MARKER_BYTES as u64);
		let mut read_marker = Vec::with_capacity(MARKER_BYTES);
		marker_file
			.take(MARKER_BYTES as u64 + 1)
			.read_to_end(&mut read_marker)?;
		assert_eq!(read_marker, marker.as_bytes());
		let mount = Self {
			root,
			volume,
			scope,
		};
		assert!(mount.free_bytes()? > PAYLOAD_BYTES as u64);
		Ok(mount)
	}

	fn free_bytes(&self) -> io::Result<u64> {
		let volume: Vec<_> = OsStr::new(&self.volume).encode_wide().chain([0]).collect();
		let (mut available, mut total, mut free) = (0, 0, 0);
		// SAFETY: the verified volume GUID is NUL-terminated and each output slot is writable.
		let ok =
			unsafe { GetDiskFreeSpaceExW(volume.as_ptr(), &mut available, &mut total, &mut free) };
		if ok == 0 {
			return Err(io::Error::last_os_error());
		}
		assert!(
			(MINIMUM_VOLUME_BYTES as u64..=VOLUME_BYTES as u64).contains(&total),
			"fixture capacity is {total} bytes"
		);
		assert_eq!(available, free, "a quota must not reduce available space");
		Ok(free)
	}
}

fn verify_volume(scope: &DirectoryScope, volume: &str, label: &str) -> Result<(), Box<dyn Error>> {
	let mut path = [0; NAME_UNITS as usize];
	// SAFETY: the held root handle and exact writable Windows path buffer are live.
	let length = unsafe {
		GetFinalPathNameByHandleW(
			scope.parent.as_raw_handle(),
			path.as_mut_ptr(),
			NAME_UNITS,
			VOLUME_NAME_GUID,
		)
	};
	assert!(
		length > 0 && length < NAME_UNITS,
		"expected a local volume GUID"
	);
	let path = String::from_utf16(&path[..length as usize])?;
	assert!(
		path.eq_ignore_ascii_case(volume),
		"mounted image volume changed"
	);

	let mut filesystem = [0; NAME_UNITS as usize];
	let mut actual_label = [0; NAME_UNITS as usize];
	// SAFETY: the held root handle and both exact writable Windows name buffers are live.
	let ok = unsafe {
		GetVolumeInformationByHandleW(
			scope.parent.as_raw_handle(),
			actual_label.as_mut_ptr(),
			NAME_UNITS,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			filesystem.as_mut_ptr(),
			NAME_UNITS,
		)
	};
	if ok == 0 {
		return Err(io::Error::last_os_error().into());
	}
	let matches_name = |buffer: &[u16], expected: &str| {
		buffer
			.iter()
			.copied()
			.take_while(|&unit| unit != 0)
			.eq(expected.encode_utf16())
	};
	assert!(matches_name(&filesystem, "NTFS"), "fixture must be NTFS");
	assert!(matches_name(&actual_label, label), "fixture label mismatch");
	Ok(())
}

fn exercise_disk_full(
	mount: &Mount,
	prepared: &Path,
	owned_scope: &DirectoryScope,
) -> Result<(), Box<dyn Error>> {
	assert_mount_is_bound(mount)?;
	let target = prepared.join(TARGET_NAME);
	let original = prepared.join(ORIGINAL_NAME);
	// prepare_reduced_fixture has already read back the actual user-only DACL on this directory.
	eprintln!("NTFS disk-full fixture: opening the ACL-verified directory under the reduced actor");
	let scope = Scope::open(&target).inspect_err(|error| {
		if matches!(error,
			StorageError::Io {
				operation: "open writable directory relative to held ancestor", source
			} if source.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed())
		) && let Err(diagnostic) = access::report(prepared, owned_scope)
		{
			eprintln!("NTFS access diagnostics incomplete: {diagnostic}");
		}
	})?;
	assert_eq!(scope.directory.identity, owned_scope.identity);
	eprintln!("NTFS disk-full fixture: verified writable scope, opening the baseline scope");
	assert!(mount.free_bytes()? > PAYLOAD_BYTES as u64);
	let original_scope = Scope::open(&original)?;
	assert_eq!(original_scope.directory.identity, scope.directory.identity);
	eprintln!("NTFS disk-full fixture: verified baseline scope, creating the baseline");
	let result = try_create_scoped(&original_scope, ORIGINAL, ORIGINAL.len(), |_| Ok(()))?;
	let CommitOutcome::CommittedVerified(original_receipt) = result else {
		panic!("create the synthetic baseline before filling: {result:?}");
	};
	let baseline = read_snapshot(&original, ORIGINAL.len())?;
	assert_eq!(baseline.fingerprint(), original_receipt.fingerprint());
	assert_eq!(baseline.bytes(), ORIGINAL);
	eprintln!("NTFS disk-full fixture: verified baseline, creating the capacity filler");
	let security = PrivateStageSecurity::current()?;
	let filler = Stage::create(&security, &scope.directory, &target)?;
	let filler_name = filler.name().to_owned();
	let headroom = fill(mount, &filler)?;

	let payload = vec![b'b'; PAYLOAD_BYTES];
	native_write_control(&security, &scope, &target, &payload)?;
	let free = mount.free_bytes()?;
	assert!((MINIMUM_HEADROOM..=HEADROOM).contains(&free));
	scope.ensure_bound()?;

	let mut phases = Vec::new();
	let outcome = try_create_scoped(&scope, &payload, payload.len(), |phase| {
		phases.push(phase);
		Ok(())
	})?;
	assert_eq!(
		phases,
		[CommitPhase::Created],
		"failure must occur during staging write"
	);
	let recovery = check_outcome(outcome, &scope, &target)?;
	let mut expected = vec![OsString::from(ORIGINAL_NAME), filler_name.clone()];
	expected.extend(recovery);
	assert_names(&scope, &mut expected)?;
	assert_original(&original, &baseline)?;

	remove_fixture_stage(filler)?;
	let recovered = mount.free_bytes()?;
	assert!(recovered > free && recovered > PAYLOAD_BYTES as u64);
	eprintln!(
		"NTFS staging headroom={headroom} bytes, reclaimed={} bytes; attempting explicit new create",
		recovered - free
	);
	scope.ensure_bound()?;
	let result = try_create_scoped(&scope, RETRY, RETRY.len(), |_| Ok(()))?;
	let CommitOutcome::CommittedVerified(receipt) = result else {
		panic!("verified retry after fixture filler removal: {result:?}");
	};
	assert!(receipt.recovery().is_none());
	let reopened = read_snapshot(&target, RETRY.len())?;
	assert_eq!(reopened.bytes(), RETRY);
	assert_eq!(reopened.fingerprint(), receipt.fingerprint());
	assert_original(&original, &baseline)?;
	expected.retain(|name| name != &filler_name);
	expected.push(OsString::from(TARGET_NAME));
	assert_names(&scope, &mut expected)?;
	Ok(())
}

fn fill(mount: &Mount, filler: &Stage) -> Result<u64, Box<dyn Error>> {
	assert_eq!(
		Observation::read(&filler.file)?.attributes,
		FILE_ATTRIBUTE_ARCHIVE
	);
	let chunk = vec![b'f'; MIB];
	let mut written = 0;
	loop {
		let free = mount.free_bytes()?;
		if free <= HEADROOM {
			assert!(
				free >= MINIMUM_HEADROOM,
				"fixture lost reserved metadata headroom"
			);
			return Ok(free);
		}
		let amount = usize::try_from((free - HEADROOM).min(MIB as u64))?;
		written += amount;
		assert!(
			written <= VOLUME_BYTES,
			"bounded filler write budget exhausted"
		);
		(&filler.file).write_all(&chunk[..amount])?;
		filler.file.sync_all()?;
	}
}

fn native_write_control(
	security: &PrivateStageSecurity,
	scope: &Scope,
	target: &Path,
	payload: &[u8],
) -> Result<(), Box<dyn Error>> {
	let control = Stage::create(security, &scope.directory, target)?;
	let prefix = [b'p'; CONTROL_PREFIX_BYTES];
	(&control.file).write_all(&prefix)?;
	control.file.sync_all()?;
	let error = (&control.file)
		.write_all(payload)
		.expect_err("independent native write must run out of NTFS capacity");
	assert_disk_full(&error);
	let length = Observation::read(&control.file)?.len;
	let requested = (prefix.len() + payload.len()) as u64;
	assert!(
		length <= requested,
		"control EOF exceeded the requested extent"
	);
	(&control.file).rewind()?;
	let mut preserved = [0; CONTROL_PREFIX_BYTES];
	(&control.file).read_exact(&mut preserved)?;
	assert_eq!(preserved, prefix, "failed write changed the synced prefix");
	eprintln!("NTFS control write error={error:?}, EOF={length} bytes; synced prefix verified");
	remove_fixture_stage(control)?;
	Ok(())
}

fn check_outcome(
	outcome: CommitOutcome,
	scope: &Scope,
	target: &Path,
) -> Result<Option<OsString>, Box<dyn Error>> {
	match outcome {
		CommitOutcome::RejectedUnchanged(error) => {
			assert_write_failure(&error);
			Ok(None)
		}
		CommitOutcome::CommitUncertain(issue) => {
			let StorageError::Io {
				operation: "cleanup rejected Windows stage",
				source,
			} = issue.error()
			else {
				panic!("uncertain result must describe an actual cleanup failure: {issue:?}");
			};
			let cleanup = source
				.get_ref()
				.and_then(|error| error.downcast_ref::<CleanupFailure>())
				.expect("cleanup must retain the original stage write error");
			assert_write_failure(&cleanup.original);
			let path = issue.recovery().path();
			assert_eq!(path.parent(), target.parent());
			assert_ne!(path, target);
			let name = path.file_name().expect("owned recovery leaf");
			let retained = scope.directory.open_file(name)?;
			let observed = Observation::read(&retained)?;
			assert_eq!(observed.identity.volume, scope.directory.identity.volume);
			assert!(observed.len <= PAYLOAD_BYTES as u64);
			PrivateStageSecurity::current()?.verify(&retained)?;
			eprintln!(
				"NTFS cleanup failure retained an owned stage with {} bytes: {:?}",
				observed.len, cleanup.cleanup
			);
			Ok(Some(name.to_owned()))
		}
		other => panic!("disk-full attempt could not publish: {other:?}"),
	}
}

fn assert_write_failure(error: &StorageError) {
	let StorageError::Io {
		operation: "write pending file",
		source,
	} = error
	else {
		panic!("expected an actual NTFS stage write error: {error:?}");
	};
	assert_disk_full(source);
	eprintln!("NTFS candidate stage write error={source:?}");
}

fn assert_disk_full(error: &io::Error) {
	assert!(
		matches!(
			error.raw_os_error().map(i32::cast_unsigned),
			Some(ERROR_DISK_FULL | ERROR_HANDLE_DISK_FULL)
		),
		"expected a native disk-full error: {error:?}"
	);
}

fn remove_fixture_stage(mut stage: Stage) -> Result<(), StorageError> {
	stage.state = State::Retained;
	dispose(&stage.file)
}

fn assert_names(scope: &Scope, expected: &mut [OsString]) -> io::Result<()> {
	let mut actual = cap_primitives::fs::read_base_dir(&scope.directory.parent)?
		.take(expected.len() + 1)
		.map(|entry| entry.map(|entry| entry.file_name()))
		.collect::<io::Result<Vec<_>>>()?;
	actual.sort_unstable();
	expected.sort_unstable();
	assert_eq!(
		actual.as_slice(),
		&*expected,
		"unexpected destination or pending file"
	);
	Ok(())
}

fn assert_original(path: &Path, baseline: &FileSnapshot) -> Result<(), StorageError> {
	let current = read_snapshot(path, ORIGINAL.len())?;
	assert_eq!(current.bytes(), baseline.bytes());
	assert_eq!(current.fingerprint(), baseline.fingerprint());
	Ok(())
}

fn assert_mount_is_bound(mount: &Mount) -> Result<(), StorageError> {
	let fresh = DirectoryScope::open_parent(&mount.root)?;
	assert_eq!(
		fresh.identity, mount.scope.identity,
		"fixture drive changed"
	);
	Ok(())
}
