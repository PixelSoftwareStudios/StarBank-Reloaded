#![cfg(target_os = "macos")]

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rustix::fs::{StatFs, XattrFlags, fstatfs, getxattr, setxattr};
use starbank_fs::{CommitIntent, CommitOutcome, StorageError, commit, read_snapshot};

const TEST_FILESYSTEM: &str = "STARBANK_ENOSPC_APFS";
const FIXTURE_PARENT: &str = "/private/tmp";
const FIXTURE_PREFIX: &str = "starbank-apfs-enospc.";
const MOUNT_NAME: &str = "volume";
const MIB: usize = 1024 * 1024;
const MIN_CAPACITY: usize = 32 * MIB;
const MAX_CAPACITY: usize = 128 * MIB;
const FILL_TARGET_FREE: usize = 8 * MIB;
const PREPARED_SIZE: usize = 16 * MIB;
const FILL_CHUNK: usize = MIB;
const EXISTING: &str = "Existing.SC2Bank";
const NEW: &str = "New.SC2Bank";
const FILLER: &str = "space-control";
const RAW_CONTROL: &str = "native-write-control";
const ORIGINAL: &[u8] = br#"<Bank version="1"/>"#;
const RETRY: &[u8] = br#"<Bank version="2"/>"#;
const MODE: u32 = 0o640;
const PERMISSION_MASK: u32 = 0o7777;
const XATTR: &str = "com.starbank.enospc-test";
const XATTR_BYTES: &[u8] = b"synthetic original metadata";
const WRITE_PENDING: &str = "write pending file";

#[test]
#[ignore = "requires the disposable APFS image from verification/macos-enospc.sh"]
fn apfs_enospc_rejects_both_commit_intents_before_publication() -> io::Result<()> {
	let (root, volume, capacity) = fixture_volume()?;
	let fixture = tempfile::tempdir_in(root)?;
	let existing = fixture.path().join(EXISTING);
	let target = fixture.path().join(NEW);
	let filler_path = fixture.path().join(FILLER);
	let control_path = fixture.path().join(RAW_CONTROL);
	fs::write(&existing, ORIGINAL)?;
	fs::set_permissions(&existing, fs::Permissions::from_mode(MODE))?;
	setxattr(&existing, XATTR, XATTR_BYTES, XattrFlags::empty())?;
	let before = read_snapshot(&existing, PREPARED_SIZE).expect("observe original bank");

	let (filled, available) = fill_volume(&volume, &filler_path)?;
	let prepared = vec![b'w'; PREPARED_SIZE];
	let mut control = create_exclusive(&control_path)?;
	let error = control
		.write_all(&prepared)
		.expect_err("native control must run out of APFS capacity");
	assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
	let control_written = control.metadata()?.len();
	drop(control);
	fs::remove_file(control_path)?;

	for (destination, intent) in [
		(&target, CommitIntent::CreateNew),
		(
			&existing,
			CommitIntent::ReplaceExpected(before.fingerprint()),
		),
	] {
		assert_staging_exhaustion(commit(destination, &prepared, intent, PREPARED_SIZE));
		let after = read_snapshot(&existing, PREPARED_SIZE).expect("reopen original bank");
		assert_eq!(after.fingerprint(), before.fingerprint());
		assert_eq!(after.bytes(), ORIGINAL);
		assert_eq!(fs::metadata(&existing)?.mode() & PERMISSION_MASK, MODE);
		let mut attribute = [0; XATTR_BYTES.len()];
		let size = getxattr(&existing, XATTR, &mut attribute)?;
		assert_eq!(&attribute[..size], XATTR_BYTES);
		let mut names = fs::read_dir(fixture.path())?
			.map(|entry| entry.map(|entry| entry.file_name()))
			.collect::<io::Result<Vec<_>>>()?;
		names.sort_unstable();
		assert_eq!(
			names,
			[OsStr::new(EXISTING), OsStr::new(FILLER)],
			"the failed create and both pending names must be absent",
		);
	}

	fs::remove_file(filler_path)?;
	let stats = fstatfs(&volume)?;
	let recovered = bytes_from_blocks(&stats, stats.f_bavail)?;
	assert!(
		recovered > available,
		"removing the filler must reclaim APFS space"
	);
	let followup = commit(&target, RETRY, CommitIntent::CreateNew, PREPARED_SIZE);
	assert!(
		matches!(followup, CommitOutcome::CommittedVerified(_)),
		"the retry must acquire the lease and publish after reclaiming space: {followup:?}",
	);
	assert_eq!(fs::read(target)?, RETRY);

	println!(
		"APFS capacity {capacity} bytes, filler {filled} bytes, available {available} bytes, raw control stored {control_written} of {PREPARED_SIZE} bytes, both staging writes errno {}, recovered {recovered} bytes",
		libc::ENOSPC,
	);
	Ok(())
}

fn fixture_volume() -> io::Result<(PathBuf, File, usize)> {
	let root = std::env::var_os(TEST_FILESYSTEM)
		.map(PathBuf::from)
		.ok_or_else(|| {
			io::Error::other(format!(
				"run verification/macos-enospc.sh ({TEST_FILESYSTEM})"
			))
		})?;
	let parent = root
		.parent()
		.ok_or_else(|| io::Error::other("missing fixture parent directory"))?;
	assert!(
		root.file_name() == Some(OsStr::new(MOUNT_NAME))
			&& parent.parent() == Some(Path::new(FIXTURE_PARENT))
			&& parent
				.file_name()
				.and_then(OsStr::to_str)
				.and_then(|name| name.strip_prefix(FIXTURE_PREFIX))
				.is_some_and(|suffix| {
					!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
				})
			&& root.canonicalize()? == root,
		"expected an unsymlinked task-owned mount path from verification/macos-enospc.sh",
	);

	let volume = File::open(&root)?;
	let stats = fstatfs(&volume)?;
	let device = c_field_bytes(&stats.f_mntfromname);
	assert!(
		c_field_bytes(&stats.f_fstypename) == b"apfs"
			&& c_field_bytes(&stats.f_mntonname) == root.as_os_str().as_bytes()
			&& device.starts_with(b"/dev/disk")
			&& volume.metadata()?.dev() != parent.metadata()?.dev()
			&& stats.f_flags & libc::MNT_LOCAL as u32 != 0,
		"fixture must be its own mounted local APFS volume",
	);
	let capacity = bytes_from_blocks(&stats, stats.f_blocks)?;
	assert!(
		(MIN_CAPACITY..=MAX_CAPACITY).contains(&capacity),
		"fixture volume capacity {capacity} is outside the dedicated image limit",
	);
	// System-created dot directories can be present even on a newly formatted volume.
	for entry in root.read_dir()? {
		let name = entry?.file_name();
		assert!(
			name.as_bytes().starts_with(b"."),
			"fixture volume has existing data"
		);
	}
	Ok((root, volume, capacity))
}

fn c_field_bytes(field: &[libc::c_char]) -> Vec<u8> {
	field
		.iter()
		.take_while(|byte| **byte != 0)
		.map(|byte| byte.to_ne_bytes()[0])
		.collect()
}

fn bytes_from_blocks(stats: &StatFs, blocks: u64) -> io::Result<usize> {
	usize::try_from(blocks)
		.ok()
		.and_then(|blocks| blocks.checked_mul(stats.f_bsize as usize))
		.ok_or_else(|| io::Error::other("APFS size overflow"))
}

fn create_exclusive(path: &Path) -> io::Result<File> {
	OpenOptions::new().write(true).create_new(true).open(path)
}

fn fill_volume(volume: &File, path: &Path) -> io::Result<(usize, usize)> {
	let mut file = create_exclusive(path)?;
	let chunk = vec![b'f'; FILL_CHUNK];
	let mut filled = 0;
	loop {
		let stats = fstatfs(volume)?;
		let available = bytes_from_blocks(&stats, stats.f_bavail)?;
		if available <= FILL_TARGET_FREE {
			assert!(
				filled > 0 && available >= MIB,
				"fixture has insufficient staging headroom"
			);
			return Ok((filled, available));
		}
		let count = chunk.len().min(available - FILL_TARGET_FREE);
		assert!(
			filled + count <= MAX_CAPACITY,
			"refuse to write beyond the fixture capacity"
		);
		file.write_all(&chunk[..count])?;
		file.sync_all()?;
		filled += count;
	}
}

fn assert_staging_exhaustion(outcome: CommitOutcome) {
	let CommitOutcome::RejectedUnchanged(StorageError::Io { operation, source }) = outcome else {
		panic!("publication must be rejected on the exhausted image: {outcome:?}");
	};
	assert_eq!(operation, WRITE_PENDING);
	assert_eq!(source.raw_os_error(), Some(libc::ENOSPC));
}
