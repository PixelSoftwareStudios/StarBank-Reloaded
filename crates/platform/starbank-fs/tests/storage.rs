#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::fs;
use std::io::{self, ErrorKind};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use starbank_fs::{
	CommitIntent, CommitOutcome, CommitReceipt, StorageError, commit, read_snapshot,
};
use tempfile::TempDir;

const LIMIT: usize = 4096;
const ORIGINAL: &[u8] = b"<Bank version=\"1\"/>";
const UPDATED: &[u8] = b"<Bank version=\"2\"/>";

struct Fixture {
	_temp: TempDir,
	path: PathBuf,
}

impl Fixture {
	fn new() -> io::Result<Self> {
		let temp = TempDir::new()?;
		// The Mac system temp root has a symlink alias. Use its physical location.
		let path = temp.path().canonicalize()?;
		Ok(Self { _temp: temp, path })
	}

	fn file(&self) -> PathBuf {
		self.path.join("Fixture.SC2Bank")
	}

	fn assert_no_pending(&self) -> io::Result<()> {
		for entry in fs::read_dir(&self.path)? {
			let name = entry?.file_name();
			assert!(
				!name.to_string_lossy().starts_with(".starbank-pending-"),
				"pending file remained after a known outcome: {name:?}"
			);
		}
		Ok(())
	}
}

fn verified(outcome: CommitOutcome) -> CommitReceipt {
	match outcome {
		CommitOutcome::CommittedVerified(receipt) => receipt,
		other => panic!("expected a verified commit, got {other:?}"),
	}
}

fn rejected(outcome: CommitOutcome) -> StorageError {
	match outcome {
		CommitOutcome::RejectedUnchanged(error) => error,
		other => panic!("expected rejection before publication, got {other:?}"),
	}
}

#[test]
fn creates_new_file_and_returns_its_fingerprint() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	let result = verified(commit(&path, ORIGINAL, CommitIntent::CreateNew, LIMIT));
	assert!(result.recovery().is_none());
	let snapshot = read_snapshot(&path, LIMIT).expect("new file must be readable");
	assert_eq!(snapshot.fingerprint(), result.fingerprint());
	assert_eq!(snapshot.into_bytes(), ORIGINAL);
	assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
	fixture.assert_no_pending()
}

#[test]
fn exclusive_creation_preserves_existing_file_and_cleans_stage() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	let before = read_snapshot(&path, LIMIT).expect("observed target");
	let error = rejected(commit(&path, UPDATED, CommitIntent::CreateNew, LIMIT));
	assert_eq!(error.kind(), ErrorKind::AlreadyExists);
	let after = read_snapshot(&path, LIMIT).expect("preserved target");
	assert_eq!(after.fingerprint(), before.fingerprint());
	assert_eq!(after.bytes(), ORIGINAL);
	fixture.assert_no_pending()
}

#[test]
fn refuses_input_over_the_declared_limit() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	let max_bytes = ORIGINAL.len() - 1;
	let error = rejected(commit(&path, ORIGINAL, CommitIntent::CreateNew, max_bytes));
	assert!(matches!(error, StorageError::TooLarge { max_bytes: maximum } if maximum == max_bytes));
	assert!(!path.exists());
	fs::write(&path, ORIGINAL)?;
	assert_eq!(
		read_snapshot(&path, max_bytes)
			.expect_err("bounded read")
			.kind(),
		ErrorKind::FileTooLarge
	);
	fixture.assert_no_pending()
}

#[test]
fn rejects_symlink_components_and_existing_symlink_destination() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	let parent_alias = fixture.path.join("ParentAlias");
	symlink(&fixture.path, &parent_alias)?;
	let aliased_output = parent_alias.join("AliasOutput.SC2Bank");
	assert!(read_snapshot(&parent_alias.join("Fixture.SC2Bank"), LIMIT).is_err());
	rejected(commit(
		&aliased_output,
		UPDATED,
		CommitIntent::CreateNew,
		LIMIT,
	));
	assert!(!aliased_output.exists());

	let leaf_alias = fixture.path.join("LeafAlias.SC2Bank");
	symlink(&path, &leaf_alias)?;
	assert!(matches!(
		read_snapshot(&leaf_alias, LIMIT),
		Err(StorageError::UnsafePath(_))
	));
	let error = rejected(commit(&leaf_alias, UPDATED, CommitIntent::CreateNew, LIMIT));
	assert_eq!(error.kind(), ErrorKind::AlreadyExists);
	assert!(fs::symlink_metadata(&leaf_alias)?.is_symlink());
	assert_eq!(fs::read(path)?, ORIGINAL);
	fixture.assert_no_pending()
}

#[test]
fn rejects_hardlinks_and_ambiguous_path_syntax() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	fs::hard_link(&path, fixture.path.join("SecondLink.SC2Bank"))?;
	assert!(matches!(
		read_snapshot(&path, LIMIT),
		Err(StorageError::UnsafePath(_))
	));
	for suffix in ["new.SC2Bank/", "new.SC2Bank/.", "../escaped.SC2Bank"] {
		let invalid = fixture.path.join(Path::new(suffix));
		assert!(matches!(
			rejected(commit(&invalid, UPDATED, CommitIntent::CreateNew, LIMIT)),
			StorageError::UnsafePath(_)
		));
	}
	assert_eq!(fs::read(path)?, ORIGINAL);
	fixture.assert_no_pending()
}

#[cfg(target_os = "macos")]
#[test]
fn overwrite_preserves_permissions_acl_xattr_and_displaced_file() -> io::Result<()> {
	use std::process::Command;

	use rustix::fs::{self as native, XattrFlags};

	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
	let acl = "group:everyone deny execute";
	let applied = Command::new("/bin/chmod")
		.arg("+a")
		.arg(acl)
		.arg(&path)
		.status()?;
	assert!(applied.success(), "fixture ACL must be installed");
	let xattr = "com.starbank.storage-test";
	native::setxattr(&path, xattr, b"original metadata", XattrFlags::empty())?;
	let before = read_snapshot(&path, LIMIT).expect("observed target");
	let result = verified(commit(
		&path,
		UPDATED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
	));
	let after = read_snapshot(&path, LIMIT).expect("observed result");
	let displaced = result.recovery().expect("overwrite retains original");
	assert_eq!(result.fingerprint(), after.fingerprint());
	assert_eq!(after.bytes(), UPDATED);
	assert_eq!(fs::read(displaced.path())?, ORIGINAL);
	for inspected in [&path, displaced.path()] {
		assert_eq!(fs::metadata(inspected)?.permissions().mode() & 0o777, 0o640);
		let mut data = [0; 32];
		let read = native::getxattr(inspected, xattr, data.as_mut_slice())?;
		assert_eq!(&data[..read], b"original metadata");
		let metadata = Command::new("/bin/ls").arg("-le").arg(inspected).output()?;
		assert!(metadata.status.success());
		assert!(String::from_utf8_lossy(&metadata.stdout).contains("deny execute"));
	}
	Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn stale_expected_bytes_or_identity_leave_the_changed_target_intact() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	let before = read_snapshot(&path, LIMIT).expect("observed target");
	let external = fixture.path.join("External.SC2Bank");
	fs::write(&external, ORIGINAL)?;
	fs::rename(external, &path)?;
	let after = read_snapshot(&path, LIMIT).expect("same bytes and changed inode");
	assert_eq!(after.fingerprint().sha256(), before.fingerprint().sha256());
	assert!(matches!(
		rejected(commit(
			&path,
			UPDATED,
			CommitIntent::ReplaceExpected(before.fingerprint()),
			LIMIT
		)),
		StorageError::Conflict
	));
	fs::write(&path, UPDATED)?;
	assert!(matches!(
		rejected(commit(
			&path,
			ORIGINAL,
			CommitIntent::ReplaceExpected(after.fingerprint()),
			LIMIT
		)),
		StorageError::Conflict
	));
	assert_eq!(fs::read(path)?, UPDATED);
	fixture.assert_no_pending()
}

#[cfg(target_os = "macos")]
#[test]
fn refuses_read_only_overwrite() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
	let before = read_snapshot(&path, LIMIT).expect("observed target");
	let error = rejected(commit(
		&path,
		UPDATED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
	));
	assert_eq!(error.kind(), ErrorKind::PermissionDenied);
	assert_eq!(fs::read(path)?, ORIGINAL);
	fixture.assert_no_pending()
}

#[cfg(target_os = "linux")]
#[test]
fn linux_overwrite_is_unavailable_before_effects() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let path = fixture.file();
	fs::write(&path, ORIGINAL)?;
	let before = read_snapshot(&path, LIMIT).expect("observed target");
	let error = rejected(commit(
		&path,
		UPDATED,
		CommitIntent::ReplaceExpected(before.fingerprint()),
		LIMIT,
	));
	assert!(matches!(error, StorageError::Unsupported(_)));
	assert_eq!(fs::read(path)?, ORIGINAL);
	fixture.assert_no_pending()
}

#[test]
fn cooperating_directory_writer_rejects_before_staging() -> io::Result<()> {
	use rustix::fs::{self as native, FlockOperation};

	let fixture = Fixture::new()?;
	let lock = fs::File::open(&fixture.path)?;
	native::flock(&lock, FlockOperation::NonBlockingLockExclusive)?;
	let target = fixture.file();
	let error = rejected(commit(&target, UPDATED, CommitIntent::CreateNew, LIMIT));
	assert_eq!(error.kind(), ErrorKind::WouldBlock);
	assert!(!target.exists());
	fixture.assert_no_pending()?;
	drop(lock);
	verified(commit(&target, UPDATED, CommitIntent::CreateNew, LIMIT));
	assert_eq!(fs::read(target)?, UPDATED);
	fixture.assert_no_pending()
}
