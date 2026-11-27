//! Synthetic qualification of the candidate without public write activation.
use super::super::rename_qualification::{prepare_reduced_fixture, with_reduced_token};
use super::{CommitPhase, Scope, commit_candidate, try_create};
use crate::{CommitOutcome, StorageError};
use sha2::{Digest as _, Sha256};
use std::fs;
use std::io;
use std::path::Path;
const TARGET: &str = "CreatedBank.SC2Bank";
const CASE_ALIAS: &str = "cREATEDbANK.sc2bank";
const PAYLOAD: &[u8] = b"<Bank><!-- complete synthetic candidate --></Bank>";
const OCCUPANT: &[u8] = b"<Bank><!-- independent occupant --></Bank>";
const MAX_BYTES: usize = 1024;

#[test]
#[ignore = "requires a native Windows ordinary local NTFS temporary directory"]
fn local_ntfs_create_candidate_transactions() -> Result<(), Box<dyn std::error::Error>> {
	let outer = tempfile::TempDir::new()?;
	let root = prepare_reduced_fixture(&outer)?;
	with_reduced_token(move || {
		verified_publication(&root)?;
		prepublication_failures_dispose_only_the_stage(&root)?;
		postpublication_failures_retain_the_destination(&root)?;
		collisions_preserve_an_independent_occupant(&root)?;
		postpublication_metadata_change_is_uncertain(&root)?;
		final_readback_lock_is_uncertain(&root)?;
		same_bytes_replacement_after_writer_close_is_uncertain(&root)?;
		bounds_and_lease_refuse_without_staging(&root)?;
		Ok(())
	})?;
	Ok(())
}

fn fixture(fixture_root: &Path) -> io::Result<(tempfile::TempDir, std::path::PathBuf)> {
	use super::super::{
		DIRECTORY_ACCESS, DirectoryScope,
		stage_security::{EffectiveToken, RelativeKind, SecurityDescriptor, create_relative},
	};
	let directory = tempfile::tempdir_in(fixture_root)?;
	let root = directory.path().canonicalize()?;
	let scope = DirectoryScope::open_parent(&root).map_err(io::Error::other)?;
	let token = EffectiveToken::current().map_err(io::Error::other)?;
	assert_eq!(
		token.selected_type().map_err(io::Error::other)?,
		windows_sys::Win32::Security::TokenImpersonation,
		"candidate fixtures require the reduced impersonation actor"
	);
	let user = token.user().map_err(io::Error::other)?;
	let group = token.primary_group().map_err(io::Error::other)?;
	let default_owner = token.default_owner().map_err(io::Error::other)?;
	eprintln!(
		"Candidate fixture default token owner differs from effective user={}",
		default_owner != user
	);
	let descriptor =
		SecurityDescriptor::from_sddl(&format!("O:{user}G:{group}D:P(A;OICI;FA;;;{user})"))
			.map_err(io::Error::other)?;
	const OWNED_DIRECTORY: &str = "CandidateFixture";
	let owned = create_relative(
		&scope.parent,
		std::ffi::OsStr::new(OWNED_DIRECTORY),
		DIRECTORY_ACCESS,
		RelativeKind::Directory,
		Some(&descriptor),
	)
	.map_err(io::Error::other)?;
	drop(owned);
	Ok((directory, root.join(OWNED_DIRECTORY)))
}

fn verified_publication(fixture_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
	for payload in [PAYLOAD, b"".as_slice()] {
		let (_directory, root) = fixture(fixture_root)?;
		let path = root.join(TARGET);
		let result = commit_candidate(&path, payload, payload.len());
		let CommitOutcome::CommittedVerified(receipt) = result else {
			panic!("verified create: {result:?}");
		};
		assert_eq!(fs::read(&path)?, payload);
		let digest: [u8; 32] = Sha256::digest(payload).into();
		assert_eq!(receipt.fingerprint().sha256(), &digest);
		assert!(receipt.recovery().is_none());
		let reopened = super::super::read_snapshot(&path, MAX_BYTES)?;
		assert_eq!(reopened.fingerprint(), receipt.fingerprint());
		assert_names(&root, &[TARGET])?;
	}
	Ok(())
}
fn prepublication_failures_dispose_only_the_stage(
	fixture_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	for phase in [
		CommitPhase::Created,
		CommitPhase::Written,
		CommitPhase::Staged,
		CommitPhase::BeforePublication,
	] {
		let (_directory, root) = fixture(fixture_root)?;
		let path = root.join(TARGET);
		let outcome = try_create(&path, PAYLOAD, MAX_BYTES, |actual| {
			if phase == actual {
				Err(injected())
			} else {
				Ok(())
			}
		})?;
		assert!(
			matches!(outcome, CommitOutcome::RejectedUnchanged(_)),
			"phase {phase:?}: {outcome:?}"
		);
		assert!(!path.exists());
		assert_names(&root, &[])?;
		assert!(
			matches!(
				commit_candidate(&path, PAYLOAD, MAX_BYTES),
				CommitOutcome::CommittedVerified(_)
			),
			"phase {phase:?}: cleanup must permit an explicit new attempt"
		);
	}
	Ok(())
}
fn postpublication_failures_retain_the_destination(
	fixture_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	for phase in [
		CommitPhase::Published,
		CommitPhase::FileSynced,
		CommitPhase::DirectorySynced,
		CommitPhase::HeldVerified,
		CommitPhase::WriterClosed,
		CommitPhase::Verified,
	] {
		let (_directory, root) = fixture(fixture_root)?;
		let path = root.join(TARGET);
		let outcome = try_create(&path, PAYLOAD, MAX_BYTES, |actual| {
			if phase == actual {
				Err(injected())
			} else {
				Ok(())
			}
		})?;
		let CommitOutcome::CommitUncertain(issue) = outcome else {
			panic!("phase {phase:?}: {outcome:?}");
		};
		assert_eq!(issue.recovery().path(), path);
		assert_eq!(fs::read(&path)?, PAYLOAD);
		assert_names(&root, &[TARGET])?;
		let before = super::super::read_snapshot(&path, MAX_BYTES)?;
		assert!(matches!(
			commit_candidate(&path, OCCUPANT, MAX_BYTES),
			CommitOutcome::RejectedUnchanged(_)
		));
		assert_eq!(
			super::super::read_snapshot(&path, MAX_BYTES)?.fingerprint(),
			before.fingerprint()
		);
	}
	Ok(())
}
fn collisions_preserve_an_independent_occupant(
	fixture_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	for (spelling, payload, late) in [
		(TARGET, OCCUPANT, false),
		(CASE_ALIAS, OCCUPANT, true),
		(TARGET, PAYLOAD, true),
	] {
		let (_directory, root) = fixture(fixture_root)?;
		let path = root.join(TARGET);
		let occupant = root.join(spelling);
		if !late {
			fs::write(&occupant, payload)?;
		}
		let mut before = if late {
			None
		} else {
			Some(super::super::read_snapshot(&occupant, MAX_BYTES)?)
		};
		let result = try_create(&path, PAYLOAD, MAX_BYTES, |phase| {
			if late && phase == CommitPhase::BeforePublication {
				fs::write(&occupant, payload)
					.map_err(|error| StorageError::io("create test competitor", error))?;
				before = Some(super::super::read_snapshot(&occupant, MAX_BYTES)?);
			}
			Ok(())
		})?;
		assert!(
			matches!(result, CommitOutcome::RejectedUnchanged(_)),
			"collision {spelling}: {result:?}"
		);
		let after = super::super::read_snapshot(&occupant, MAX_BYTES)?;
		assert_eq!(
			after.fingerprint(),
			before.expect("fixture created competitor").fingerprint()
		);
		assert_eq!(after.bytes(), payload);
		assert_names(&root, &[spelling])?;
	}
	Ok(())
}
fn postpublication_metadata_change_is_uncertain(
	fixture_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let (_directory, root) = fixture(fixture_root)?;
	let path = root.join(TARGET);
	let mut initial_permissions = None;
	let result = try_create(&path, PAYLOAD, MAX_BYTES, |phase| {
		if phase == CommitPhase::DirectorySynced {
			let mut permissions = fs::metadata(&path)
				.map_err(|error| StorageError::io("read test metadata", error))?
				.permissions();
			initial_permissions = Some(permissions.clone());
			permissions.set_readonly(true);
			fs::set_permissions(&path, permissions)
				.map_err(|error| StorageError::io("change test metadata", error))?;
		}
		Ok(())
	})?;
	let CommitOutcome::CommitUncertain(issue) = result else {
		panic!("metadata change: {result:?}");
	};
	assert_eq!(issue.recovery().path(), path);
	assert_eq!(fs::read(&path)?, PAYLOAD);
	assert_names(&root, &[TARGET])?;
	fs::set_permissions(
		&path,
		initial_permissions.expect("fixture captured original permissions"),
	)?;
	Ok(())
}

fn bounds_and_lease_refuse_without_staging(
	fixture_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let (_directory, root) = fixture(fixture_root)?;
	let path = root.join(TARGET);
	assert!(matches!(
		commit_candidate(&path, PAYLOAD, PAYLOAD.len() - 1),
		CommitOutcome::RejectedUnchanged(StorageError::TooLarge { .. })
	));
	assert_names(&root, &[])?;
	let scope = Scope::open(&path)?;
	let _lease = super::Lease::acquire(scope.directory.identity)?;
	let (send, receive) = std::sync::mpsc::channel();
	let worker_path = path.clone();
	let worker = std::thread::spawn(move || {
		with_reduced_token(move || {
			send.send(commit_candidate(&worker_path, PAYLOAD, MAX_BYTES))
				.map_err(io::Error::other)?;
			Ok(())
		})
	});
	worker.join().expect("candidate worker")?;
	let blocked = receive.recv()?;
	assert!(
		matches!(blocked, CommitOutcome::RejectedUnchanged(StorageError::Io { source, .. }) if source.kind() == io::ErrorKind::ResourceBusy)
	);
	assert_names(&root, &[])?;
	Ok(())
}
fn injected() -> StorageError {
	StorageError::io(
		"test checkpoint",
		io::Error::other("synthetic phase failure"),
	)
}
fn assert_names(root: &Path, expected: &[&str]) -> io::Result<()> {
	let mut names = fs::read_dir(root)?
		.map(|entry| entry.map(|entry| entry.file_name()))
		.collect::<io::Result<Vec<_>>>()?;
	names.sort_unstable();
	let mut expected: Vec<_> = expected.iter().map(std::ffi::OsString::from).collect();
	expected.sort_unstable();
	assert_eq!(names, expected, "unexpected pending file or destination");
	Ok(())
}

fn final_readback_lock_is_uncertain(fixture_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
	use std::os::windows::fs::OpenOptionsExt as _;
	let (_directory, root) = fixture(fixture_root)?;
	let path = root.join(TARGET);
	let mut blocker = None;
	let result = try_create(&path, PAYLOAD, MAX_BYTES, |phase| {
		if phase == CommitPhase::WriterClosed {
			blocker = Some(
				fs::OpenOptions::new()
					.read(true)
					.share_mode(0)
					.open(&path)
					.map_err(|error| StorageError::io("lock final readback fixture", error))?,
			);
		}
		Ok(())
	})?;
	let CommitOutcome::CommitUncertain(issue) = result else {
		panic!("readback lock: {result:?}");
	};
	assert_eq!(issue.recovery().path(), path);
	drop(blocker);
	assert_eq!(fs::read(&path)?, PAYLOAD);
	assert_names(&root, &[TARGET])?;
	Ok(())
}
fn same_bytes_replacement_after_writer_close_is_uncertain(
	fixture_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	const DISPLACED: &str = "ExternalMoved.SC2Bank";
	let (_directory, root) = fixture(fixture_root)?;
	let path = root.join(TARGET);
	let displaced = root.join(DISPLACED);
	let mut published_identity = None;
	let result = try_create(&path, PAYLOAD, MAX_BYTES, |phase| {
		if phase == CommitPhase::WriterClosed {
			published_identity = Some(
				super::super::read_snapshot(&path, MAX_BYTES)?
					.fingerprint()
					.windows
					.file
					.identity,
			);
			fs::rename(&path, &displaced)
				.map_err(|error| StorageError::io("move readback fixture", error))?;
			fs::write(&path, PAYLOAD)
				.map_err(|error| StorageError::io("replace readback fixture", error))?;
		}
		Ok(())
	})?;
	let CommitOutcome::CommitUncertain(issue) = result else {
		panic!("same-byte replacement: {result:?}");
	};
	assert_eq!(issue.recovery().path(), path);
	let current = super::super::read_snapshot(&path, MAX_BYTES)?;
	assert_ne!(
		Some(current.fingerprint().windows.file.identity),
		published_identity
	);
	assert_eq!(current.bytes(), PAYLOAD);
	assert_eq!(fs::read(&displaced)?, PAYLOAD);
	assert_names(&root, &[TARGET, DISPLACED])?;
	Ok(())
}
