#![cfg(target_os = "linux")]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

use rustix::fs::{FsWord, fstatfs};
use rustix::io::Errno;
use starbank_fs::{CommitIntent, CommitOutcome, StorageError, commit, read_snapshot};

const TEST_FILESYSTEM: &str = "STARBANK_ENOSPC_TMPFS";
const TMPFS_MAGIC: FsWord = 0x0102_1994;
const MAX_CAPACITY: usize = 1024 * 1024;
const EXISTING: &str = "Existing.SC2Bank";
const NEW: &str = "New.SC2Bank";
const CONTROL: &str = "write-control";
const OLD_BYTES: &[u8] = br#"<Bank version="1"/>"#;
const NEW_BYTES: &[u8] = br#"<Bank version="2"/>"#;
const WRITE_PENDING: &str = "write pending file";

#[test]
#[ignore = "requires an empty dedicated Linux tmpfs capped at 1 MiB or less"]
fn tmpfs_enospc_rejects_before_publication_and_releases_space_and_lease() -> io::Result<()> {
	let root = std::env::var_os(TEST_FILESYSTEM)
		.map(PathBuf::from)
		.ok_or_else(|| io::Error::other(format!("set {TEST_FILESYSTEM} to the fixture mount")))?
		.canonicalize()?;
	let root_handle = File::open(&root)?;
	let stats = fstatfs(&root_handle)?;
	assert_eq!(
		stats.f_type, TMPFS_MAGIC,
		"fixture filesystem must be tmpfs"
	);
	let block_size = usize::try_from(stats.f_bsize).map_err(io::Error::other)?;
	let total_blocks = usize::try_from(stats.f_blocks).map_err(io::Error::other)?;
	let capacity = block_size
		.checked_mul(total_blocks)
		.ok_or_else(|| io::Error::other("fixture capacity overflow"))?;
	assert!(
		total_blocks >= 2 && (1..=MAX_CAPACITY).contains(&capacity),
		"fixture requires at least two blocks and at most {MAX_CAPACITY} bytes",
	);
	assert!(
		stats.f_bavail == stats.f_blocks && fs::read_dir(&root)?.next().transpose()?.is_none(),
		"fixture filesystem must be dedicated and empty",
	);

	let fixture = tempfile::tempdir_in(root)?;
	let existing = fixture.path().join(EXISTING);
	let target = fixture.path().join(NEW);
	let control_path = fixture.path().join(CONTROL);
	fs::write(&existing, OLD_BYTES)?;
	let before = read_snapshot(&existing, capacity).expect("capture original bank");
	let free_before = fstatfs(&root_handle)?.f_bavail;

	let prepared = vec![b'x'; capacity + 1];
	let mut control = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&control_path)?;
	let written = control.write(&prepared)?;
	assert!(
		(1..prepared.len()).contains(&written),
		"control must make a real partial write",
	);
	let error = control
		.write(&prepared[written..])
		.expect_err("control must exhaust available tmpfs blocks");
	assert_eq!(error.raw_os_error(), Some(Errno::NOSPC.raw_os_error()));
	assert_eq!(fstatfs(&root_handle)?.f_bavail, 0);
	drop(control);
	fs::remove_file(control_path)?;
	assert_eq!(fstatfs(&root_handle)?.f_bavail, free_before);

	let outcome = commit(&target, &prepared, CommitIntent::CreateNew, prepared.len());
	let CommitOutcome::RejectedUnchanged(StorageError::Io { operation, source }) = outcome else {
		panic!("the creation must fail while writing its stage: {outcome:?}");
	};
	assert_eq!(operation, WRITE_PENDING);
	assert_eq!(source.raw_os_error(), Some(Errno::NOSPC.raw_os_error()));

	let after = read_snapshot(&existing, capacity).expect("reopen original bank");
	assert_eq!(after.fingerprint(), before.fingerprint());
	assert_eq!(after.bytes(), OLD_BYTES);
	assert!(!target.exists());
	let entries = fs::read_dir(fixture.path())?.collect::<io::Result<Vec<_>>>()?;
	assert_eq!(entries.len(), 1, "the failed staging name must be removed");
	assert_eq!(fstatfs(&root_handle)?.f_bavail, free_before);

	let followup = commit(&target, NEW_BYTES, CommitIntent::CreateNew, capacity);
	assert!(
		matches!(followup, CommitOutcome::CommittedVerified(_)),
		"a new attempt must acquire the lease and have space: {followup:?}",
	);
	assert_eq!(fs::read(target)?, NEW_BYTES);

	println!(
		"tmpfs capacity {capacity} bytes, control wrote {written} of {} bytes, staging write errno {}",
		prepared.len(),
		Errno::NOSPC.raw_os_error(),
	);
	Ok(())
}
