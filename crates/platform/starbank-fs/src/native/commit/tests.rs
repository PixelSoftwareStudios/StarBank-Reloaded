use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use tempfile::TempDir;

use crate::{CommitIntent, CommitOutcome, StorageError, read_snapshot};

use super::{CommitPhase, try_commit};

const ORIGINAL: &[u8] = br#"<Bank version="1"/>"#;
const PREPARED: &[u8] = br#"<Bank version="2"/>"#;
const EXTERNAL: &[u8] = br#"<Bank version="3"/>"#;
const LIMIT: usize = 4096;
const PENDING_PREFIX: &str = ".starbank-pending-";
const FIXTURE_NAME: &str = "CrashFixture.SC2Bank";
const CHILD_TEST: &str = "native::commit::tests::pause_at_checkpoint";
const CHILD_ROOT: &str = "STARBANK_STORAGE_TEST_ROOT";
const CHILD_PHASE: &str = "STARBANK_STORAGE_TEST_PHASE";
const CHILD_READY: &str = "STARBANK_STORAGE_TEST_CHECKPOINT";
const CHILD_TIMEOUT: Duration = Duration::from_secs(15);
const STAGED_PHASE: &str = "staged";
const PUBLISHED_PHASE: &str = "published";
const DIRECTORY_SYNCED_PHASE: &str = "directory-synced";

struct Fixture {
	_directory: TempDir,
	root: PathBuf,
	target: PathBuf,
}

impl Fixture {
	fn new() -> io::Result<Self> {
		let directory = TempDir::new()?;
		let root = directory.path().canonicalize()?;
		let target = root.join(FIXTURE_NAME);
		fs::write(&target, ORIGINAL)?;
		Ok(Self {
			_directory: directory,
			root,
			target,
		})
	}

	fn pending_paths(&self) -> io::Result<Vec<PathBuf>> {
		let mut pending = Vec::new();
		for entry in fs::read_dir(&self.root)? {
			let entry = entry?;
			if entry
				.file_name()
				.to_string_lossy()
				.starts_with(PENDING_PREFIX)
			{
				pending.push(entry.path());
			}
		}
		Ok(pending)
	}
}

fn injected_fault() -> StorageError {
	StorageError::io(
		"injected storage fault",
		io::Error::other("qualification fixture"),
	)
}

#[test]
fn failure_after_stage_sync_leaves_original_and_removes_owned_stage() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let before = read_snapshot(&fixture.target, LIMIT).expect("original file is observable");
	let error = try_commit(
		&fixture.target,
		PREPARED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
		None,
		|checkpoint| {
			assert_eq!(checkpoint, CommitPhase::Staged);
			assert_eq!(fs::read(&fixture.target).expect("original bytes"), ORIGINAL);
			assert_eq!(fixture.pending_paths().expect("one staged entry").len(), 1);
			Err(injected_fault())
		},
	)
	.expect_err("a pre-publication fault must reject publication");
	assert!(matches!(
		error,
		StorageError::Io {
			operation: "injected storage fault",
			..
		}
	));
	let after = read_snapshot(&fixture.target, LIMIT).expect("target after rejected write");
	assert_eq!(after.fingerprint(), before.fingerprint());
	assert!(fixture.pending_paths()?.is_empty());
	Ok(())
}

#[test]
fn ordinary_overwrite_rechecks_the_parent_binding_before_publication() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let chosen = fixture.root.join("Selected");
	let moved = fixture.root.join("Original");
	fs::create_dir(&chosen)?;
	let target = chosen.join(FIXTURE_NAME);
	fs::write(&target, ORIGINAL)?;
	let before = read_snapshot(&target, LIMIT).expect("read original fixture");
	let rejected = try_commit(
		&target,
		PREPARED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
		None,
		|phase| {
			if phase == CommitPhase::Staged {
				fs::rename(&chosen, &moved)
					.and_then(|()| fs::create_dir(&chosen))
					.and_then(|()| fs::write(&target, EXTERNAL))
					.map_err(|error| StorageError::io("replace fixture directory", error))?;
			}
			Ok(())
		},
	)
	.expect_err("a moved destination must be rejected before writing the bank");
	assert!(matches!(rejected, StorageError::Conflict));
	assert_eq!(fs::read(&target)?, EXTERNAL);
	assert_eq!(fs::read(moved.join(FIXTURE_NAME))?, ORIGINAL);
	assert_eq!(fs::read_dir(moved)?.count(), 1);
	Ok(())
}

#[test]
fn ordinary_overwrite_rechecks_the_pending_file_before_publication() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let before = read_snapshot(&fixture.target, LIMIT).expect("read original fixture");
	let rejected = try_commit(
		&fixture.target,
		PREPARED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
		None,
		|phase| {
			if phase == CommitPhase::Staged {
				let pending = fixture
					.pending_paths()
					.map_err(|error| StorageError::io("find fixture stage", error))?;
				assert_eq!(pending.len(), 1);
				fs::write(&pending[0], b"changed pending bytes")
					.map_err(|error| StorageError::io("change fixture stage", error))?;
			}
			Ok(())
		},
	)
	.expect_err("a changed stage must be rejected before writing the bank");
	assert!(matches!(rejected, StorageError::Conflict));
	let after = read_snapshot(&fixture.target, LIMIT).expect("read rejected fixture");
	assert_eq!(before.fingerprint(), after.fingerprint());
	assert_eq!(after.bytes(), ORIGINAL);
	assert!(fixture.pending_paths()?.is_empty());
	Ok(())
}

#[test]
fn cancel_before_directory_sync_or_readback_preserves_recovery() -> io::Result<()> {
	for at in [CommitPhase::Published, CommitPhase::DirectorySynced] {
		let fixture = Fixture::new()?;
		let before = read_snapshot(&fixture.target, LIMIT).expect("original file is observable");
		let outcome = try_commit(
			&fixture.target,
			PREPARED,
			CommitIntent::ReplaceExpected(before.fingerprint()),
			LIMIT,
			None,
			|checkpoint| {
				if checkpoint == at {
					Err(injected_fault())
				} else {
					Ok(())
				}
			},
		)
		.expect("post-publication faults produce a distinct outcome");
		let CommitOutcome::CommitUncertain(issue) = outcome else {
			panic!("expected uncertain at {at:?}, got {outcome:?}");
		};
		assert!(matches!(
			issue.error(),
			StorageError::Io {
				operation: "injected storage fault",
				..
			}
		));
		assert_eq!(fs::read(&fixture.target)?, PREPARED);
		assert_eq!(fs::read(issue.recovery().path())?, ORIGINAL);
	}
	Ok(())
}

#[test]
fn external_create_wins_atomic_no_replace() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let target = fixture.root.join("BrandNew.SC2Bank");
	let error = try_commit(
		&target,
		PREPARED,
		CommitIntent::CreateNew,
		LIMIT,
		None,
		|checkpoint| {
			assert_eq!(checkpoint, CommitPhase::Staged);
			fs::write(&target, EXTERNAL)
				.map_err(|error| StorageError::io("publish competing fixture", error))
		},
	)
	.expect_err("competing creator must win");
	assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
	assert_eq!(fs::read(target)?, EXTERNAL);
	assert!(fixture.pending_paths()?.is_empty());
	Ok(())
}

#[test]
fn external_replacement_after_final_check_retains_both_versions() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let competing = fixture.root.join("Competing.SC2Bank");
	fs::write(&competing, EXTERNAL)?;
	let expected = read_snapshot(&fixture.target, LIMIT).expect("original file is observable");
	let outcome = try_commit(
		&fixture.target,
		PREPARED,
		CommitIntent::ReplaceExpected(expected.fingerprint()),
		LIMIT,
		None,
		|checkpoint| {
			if checkpoint == CommitPhase::Staged {
				fs::rename(&competing, &fixture.target)
					.map_err(|error| StorageError::io("publish competing fixture", error))?;
			}
			Ok(())
		},
	)
	.expect("late conflict must be reported as an outcome");
	let CommitOutcome::CommitUncertain(issue) = outcome else {
		panic!("expected an uncertain publication, got {outcome:?}");
	};
	assert!(matches!(issue.error(), StorageError::Conflict));
	assert_eq!(fs::read(&fixture.target)?, PREPARED);
	assert_eq!(fs::read(issue.recovery().path())?, EXTERNAL);
	Ok(())
}

#[test]
fn post_publication_data_change_is_an_uncertain_outcome() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let before = read_snapshot(&fixture.target, LIMIT).expect("original file is observable");
	let outcome = try_commit(
		&fixture.target,
		PREPARED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
		None,
		|checkpoint| {
			if checkpoint == CommitPhase::Published {
				fs::write(&fixture.target, EXTERNAL)
					.map_err(|error| StorageError::io("change published fixture", error))?;
			}
			Ok(())
		},
	)
	.expect("post-publication changes produce a distinct outcome");
	let CommitOutcome::CommitUncertain(issue) = outcome else {
		panic!("expected uncertainty after verified content changed, got {outcome:?}");
	};
	assert!(matches!(issue.error(), StorageError::Conflict));
	assert_eq!(fs::read(&fixture.target)?, EXTERNAL);
	assert_eq!(fs::read(issue.recovery().path())?, ORIGINAL);
	Ok(())
}

#[test]
fn process_death_at_apfs_commit_boundaries_retains_bytes_and_releases_lease() -> io::Result<()> {
	for (phase, expected_target, expected_sibling) in [
		(STAGED_PHASE, ORIGINAL, PREPARED),
		(PUBLISHED_PHASE, PREPARED, ORIGINAL),
		(DIRECTORY_SYNCED_PHASE, PREPARED, ORIGINAL),
	] {
		let fixture = Fixture::new()?;
		let mut worker = PausedWorker::start(&fixture.root, phase)?;
		assert_eq!(fs::read(&fixture.target)?, expected_target, "at {phase}");
		let concurrent_target = fixture.root.join("Concurrent.SC2Bank");
		let outcome = super::commit(&concurrent_target, EXTERNAL, CommitIntent::CreateNew, LIMIT);
		assert!(
			matches!(outcome, CommitOutcome::RejectedUnchanged(error)
			if error.kind() == io::ErrorKind::WouldBlock),
			"at {phase}"
		);
		assert!(
			!concurrent_target.exists(),
			"busy writer had no target effect"
		);
		assert_eq!(
			fixture.pending_paths()?.len(),
			1,
			"busy writer left no stage"
		);
		worker.kill()?;

		let reopened = read_snapshot(&fixture.target, LIMIT).expect("target readable after death");
		assert_eq!(reopened.bytes(), expected_target, "at {phase}");
		let pending = fixture.pending_paths()?;
		assert_eq!(pending.len(), 1, "one artifact remains at {phase}");
		assert_eq!(fs::read(&pending[0])?, expected_sibling, "at {phase}");
		let next = super::commit(&concurrent_target, EXTERNAL, CommitIntent::CreateNew, LIMIT);
		assert!(
			matches!(next, CommitOutcome::CommittedVerified(_)),
			"lease released at {phase}: {next:?}"
		);
		assert_eq!(fs::read(concurrent_target)?, EXTERNAL);
	}
	Ok(())
}

struct PausedWorker(Child);

impl PausedWorker {
	fn start(root: &Path, phase: &str) -> io::Result<Self> {
		let mut worker = Self(
			Command::new(std::env::current_exe()?)
				.args([CHILD_TEST, "--exact", "--ignored", "--nocapture"])
				.env(CHILD_ROOT, root)
				.env(CHILD_PHASE, phase)
				.stdin(Stdio::piped())
				.stdout(Stdio::piped())
				.spawn()?,
		);
		let output = worker
			.0
			.stdout
			.take()
			.ok_or_else(|| io::Error::other("missing worker pipe"))?;
		let (signal, ready) = mpsc::sync_channel(1);
		std::thread::spawn(move || {
			for line in BufReader::new(output).lines().map_while(Result::ok) {
				if line.contains(CHILD_READY) {
					let _ = signal.send(());
					return;
				}
			}
		});
		ready
			.recv_timeout(CHILD_TIMEOUT)
			.map_err(io::Error::other)?;
		Ok(worker)
	}

	fn kill(&mut self) -> io::Result<()> {
		// Child::wait closes stdin; keep the checkpoint pause until the child is reaped.
		let _pause = self.0.stdin.take();
		self.0.kill()?;
		let status = self.0.wait()?;
		assert_eq!(
			status.signal(),
			Some(libc::SIGKILL),
			"worker killed at checkpoint"
		);
		Ok(())
	}
}

impl Drop for PausedWorker {
	fn drop(&mut self) {
		let _pause = self.0.stdin.take();
		if !matches!(self.0.try_wait(), Ok(Some(_))) {
			let _ = self.0.kill();
			let _ = self.0.wait();
		}
	}
}

#[test]
#[ignore = "worker launched by process_death_at_apfs_commit_boundaries_retains_bytes_and_releases_lease"]
fn pause_at_checkpoint() -> io::Result<()> {
	let Some(root) = std::env::var_os(CHILD_ROOT) else {
		return Ok(());
	};
	let selected = match std::env::var(CHILD_PHASE).as_deref() {
		Ok(STAGED_PHASE) => CommitPhase::Staged,
		Ok(PUBLISHED_PHASE) => CommitPhase::Published,
		Ok(DIRECTORY_SYNCED_PHASE) => CommitPhase::DirectorySynced,
		_ => return Err(io::Error::other("missing worker checkpoint")),
	};
	let target = PathBuf::from(root).join(FIXTURE_NAME);
	let before = read_snapshot(&target, LIMIT).expect("read the synthetic worker fixture");
	assert_eq!(
		before.bytes(),
		ORIGINAL,
		"launcher supplies original fixture"
	);
	let outcome = try_commit(
		&target,
		PREPARED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
		None,
		|checkpoint| {
			if checkpoint == selected {
				let mut stdout = io::stdout().lock();
				writeln!(stdout, "{CHILD_READY}")
					.and_then(|()| stdout.flush())
					.and_then(|()| io::stdin().read_exact(&mut [0]))
					.map_err(|error| StorageError::io("hold child at checkpoint", error))?;
			}
			Ok(())
		},
	);
	Err(io::Error::other(format!(
		"worker left {selected:?} checkpoint: {outcome:?}"
	)))
}
