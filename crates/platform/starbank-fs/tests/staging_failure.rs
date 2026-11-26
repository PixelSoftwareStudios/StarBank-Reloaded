#![cfg(target_os = "macos")]

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use starbank_fs::{CommitIntent, CommitOutcome, StorageError, commit, read_snapshot};
use tempfile::TempDir;

const WORKER_TEST: &str = "staging_limit_worker";
const WORKER_ROOT: &str = "STARBANK_APFS_LIMIT_TEST_DIRECTORY";
const WORKER_PASS: &str = "STARBANK_APFS_LIMIT_TEST_PASSED";
const LIMIT_BLOCKS: &str = "1";
const EXISTING_NAME: &str = "Existing.SC2Bank";
const NEW_NAME: &str = "New.SC2Bank";
const CONTROL_NAME: &str = "limit-control";
const ORIGINAL: &[u8] = br#"<Bank version="1"/>"#;
const RETRY: &[u8] = br#"<Bank version="2"/>"#;
const PAYLOAD_BYTES: usize = 8192;
const MAX_BYTES: usize = 16 * 1024;
const WRITE_PENDING: &str = "write pending file";

#[test]
fn native_short_write_rejects_before_publication_and_cleans_stage() -> io::Result<()> {
	let temporary = TempDir::new()?;
	let root = temporary.path().canonicalize()?;
	let existing = root.join(EXISTING_NAME);
	let new = root.join(NEW_NAME);
	fs::write(&existing, ORIGINAL)?;
	let before = read_snapshot(&existing, MAX_BYTES).expect("observe original before failures");

	// Only the fixture process inherits the file-size limit and ignores SIGXFSZ. All paths and
	// arguments are passed as shell parameters and the shell program is fixed.
	let child = Command::new("/bin/sh")
		.args([
			"-c",
			r#"ulimit -f "$1" && trap '' XFSZ && shift && exec "$@""#,
			"starbank-staging-limit",
			LIMIT_BLOCKS,
		])
		.arg(std::env::current_exe()?)
		.args([WORKER_TEST, "--exact", "--ignored", "--nocapture"])
		.env(WORKER_ROOT, &root)
		.output()?;
	let stdout = String::from_utf8_lossy(&child.stdout);
	let stderr = String::from_utf8_lossy(&child.stderr);
	assert!(
		child.status.success() && stdout.contains(WORKER_PASS),
		"file-size-limited process did not verify the native error: {:?}\n{stdout}\n{stderr}",
		child.status,
	);

	let after = read_snapshot(&existing, MAX_BYTES).expect("observe original after failures");
	assert_eq!(after.fingerprint(), before.fingerprint());
	assert_eq!(after.bytes(), ORIGINAL);
	assert!(
		!new.exists(),
		"failed create must leave the destination absent"
	);
	let entries = fs::read_dir(&root)?.collect::<io::Result<Vec<_>>>()?;
	assert_eq!(entries.len(), 1, "failed stages must be removed");

	let followup = commit(&new, RETRY, CommitIntent::CreateNew, MAX_BYTES);
	assert!(
		matches!(followup, CommitOutcome::CommittedVerified(_)),
		"fresh writer must acquire the directory lease: {followup:?}"
	);
	assert_eq!(fs::read(new)?, RETRY);
	Ok(())
}

#[test]
#[ignore = "launched by the native short-write test with a child-only file-size limit"]
fn staging_limit_worker() -> io::Result<()> {
	let Some(root) = std::env::var_os(WORKER_ROOT).map(PathBuf::from) else {
		return Ok(());
	};
	let existing = root.join(EXISTING_NAME);
	let snapshot = read_snapshot(&existing, MAX_BYTES).expect("open owned synthetic fixture");
	assert_eq!(snapshot.bytes(), ORIGINAL);

	let prepared = [b'w'; PAYLOAD_BYTES];
	let control_path = root.join(CONTROL_NAME);
	let mut control = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&control_path)?;
	let written = control.write(&prepared)?;
	assert!(
		(1..prepared.len()).contains(&written),
		"the native control must make a real partial write"
	);
	let error = control
		.write(&prepared[written..])
		.expect_err("the native control must hit its file-size limit");
	assert_eq!(error.raw_os_error(), Some(libc::EFBIG));
	drop(control);
	fs::remove_file(control_path)?;

	for (target, intent) in [
		(root.join(NEW_NAME), CommitIntent::CreateNew),
		(
			existing,
			CommitIntent::ReplaceExpected(snapshot.fingerprint()),
		),
	] {
		let outcome = commit(&target, &prepared, intent, MAX_BYTES);
		let CommitOutcome::RejectedUnchanged(StorageError::Io { operation, source }) = outcome
		else {
			panic!("native staging error must reject before publication: {outcome:?}");
		};
		assert_eq!(operation, WRITE_PENDING);
		assert_eq!(source.raw_os_error(), Some(libc::EFBIG));
	}
	println!("{WORKER_PASS}");
	Ok(())
}
