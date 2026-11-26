#![cfg(windows)]

use std::fs::{self, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use starbank_fs::{CommitIntent, CommitOutcome, StorageError, commit, read_snapshot};
use windows_sys::Win32::{
	Foundation::ERROR_SHARING_VIOLATION,
	Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE},
};

const MAX_BYTES: usize = 4096;
const ABC_SHA_256: [u8; 32] = [
	0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
	0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
];

#[test]
fn bounded_snapshot_uses_identity_and_exact_bytes() -> io::Result<()> {
	let directory = tempfile::TempDir::new()?;
	let file = directory.path().join("Synthetic.SC2Bank");
	fs::write(&file, b"abc")?;
	let first = read_snapshot(&file, 3).expect("read ordinary local file");
	let second = read_snapshot(&file, 3).expect("read same local file again");
	assert_eq!(first.fingerprint(), second.fingerprint());
	assert_eq!(first.bytes(), b"abc");
	assert_eq!(*first.fingerprint().sha256(), ABC_SHA_256);
	assert!(matches!(
		read_snapshot(&file, 2),
		Err(StorageError::TooLarge { max_bytes: 2 })
	));

	let other = directory.path().join("SameBytes.SC2Bank");
	fs::write(&other, b"abc")?;
	fs::remove_file(&file)?;
	fs::rename(other, &file)?;
	let swapped = read_snapshot(&file, 3).expect("read different file at same path");
	assert_eq!(first.fingerprint().sha256(), swapped.fingerprint().sha256());
	assert_ne!(first.fingerprint(), swapped.fingerprint());
	Ok(())
}

#[test]
fn refuses_to_read_file_held_open_for_writing() -> io::Result<()> {
	let directory = tempfile::TempDir::new()?;
	let file = directory.path().join("Sharing.SC2Bank");
	fs::write(&file, b"synthetic")?;
	let writer = OpenOptions::new()
		.write(true)
		.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
		.open(&file)?;
	let error = read_snapshot(&file, MAX_BYTES).expect_err("concurrent writer must be refused");
	assert!(matches!(error, StorageError::Io { source, .. }
		if source.raw_os_error() == Some(ERROR_SHARING_VIOLATION.cast_signed())));
	drop(writer);
	assert_eq!(
		read_snapshot(&file, MAX_BYTES)
			.expect("snapshot after writer closed")
			.bytes(),
		b"synthetic"
	);
	Ok(())
}

#[test]
fn rejects_hardlinks_and_directory_targets() -> io::Result<()> {
	let directory = tempfile::TempDir::new()?;
	let file = directory.path().join("Source.SC2Bank");
	fs::write(&file, b"synthetic")?;
	fs::hard_link(&file, directory.path().join("Alias.SC2Bank"))?;
	assert!(matches!(
		read_snapshot(&file, MAX_BYTES),
		Err(StorageError::UnsafePath(_))
	));
	assert!(read_snapshot(directory.path(), MAX_BYTES).is_err());
	Ok(())
}

#[test]
fn refuses_windows_names_that_do_not_select_an_ordinary_file() {
	for input in [
		r"C:relative.SC2Bank",
		r"\DriveRelative.SC2Bank",
		r"C:\Folder\..\Traversal.SC2Bank",
		r"\\example.invalid\share\Remote.SC2Bank",
		r"\\.\pipe\NonDisk.SC2Bank",
		r"C:\Folder\Alternate.SC2Bank:alternate",
		r"C:\Folder\Trailing.SC2Bank\",
		r"C:\Folder\Trailing.SC2Bank.",
		r"C:\Folder\Trailing.SC2Bank\.",
	] {
		assert!(
			matches!(
				read_snapshot(Path::new(input), MAX_BYTES),
				Err(StorageError::UnsafePath(_))
			),
			"path must be rejected with no local file content read: {input}",
		);
	}
}

#[test]
#[ignore = "requires permission to create Windows symbolic links"]
fn rejects_file_and_parent_reparse_points() -> io::Result<()> {
	use std::os::windows::fs::{symlink_dir, symlink_file};

	let directory = tempfile::TempDir::new()?;
	let child = directory.path().join("Real");
	fs::create_dir(&child)?;
	let file = child.join("Source.SC2Bank");
	fs::write(&file, b"synthetic")?;
	let alias = directory.path().join("Alias.SC2Bank");
	symlink_file(&file, &alias)?;
	assert!(read_snapshot(&alias, MAX_BYTES).is_err());
	let parent_alias = directory.path().join("DirectoryAlias");
	symlink_dir(child, &parent_alias)?;
	assert!(read_snapshot(&parent_alias.join("Source.SC2Bank"), MAX_BYTES).is_err());
	Ok(())
}

#[test]
fn windows_commit_rejects_before_opening_the_target() -> io::Result<()> {
	let directory = tempfile::TempDir::new()?;
	let file = directory.path().join("Destination.SC2Bank");
	assert!(matches!(
		commit(&file, b"synthetic", CommitIntent::CreateNew, MAX_BYTES),
		CommitOutcome::RejectedUnchanged(StorageError::Unsupported(_))
	));
	assert!(!file.exists());
	Ok(())
}
