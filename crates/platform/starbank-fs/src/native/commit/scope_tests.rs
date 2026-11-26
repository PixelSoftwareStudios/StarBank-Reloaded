use std::{error::Error, fs};

use super::*;

#[test]
fn replaced_managed_directory_after_staging_cannot_receive_a_payload() -> Result<(), Box<dyn Error>>
{
	let sandbox = tempfile::tempdir()?;
	let root = sandbox.path().canonicalize()?;
	let chosen = root.join("Selected");
	let moved = root.join("Original");
	fs::create_dir(&chosen)?;
	let scope = BackupDirectory::capture(&chosen)?;
	let error = try_commit(
		&chosen.join("Fixture.sc2bank.bak"),
		b"<Bank/>",
		CommitIntent::CreateNew,
		4096,
		Some(&scope),
		|phase| {
			assert_eq!(phase, CommitPhase::Staged);
			assert_eq!(fs::read_dir(&chosen).expect("staged directory").count(), 1);
			fs::rename(&chosen, &moved)
				.and_then(|()| fs::create_dir(&chosen))
				.map_err(|error| StorageError::io("replace managed fixture directory", error))
		},
	)
	.expect_err("managed parent identity changed before publication");
	assert!(matches!(error, StorageError::Conflict));
	assert_eq!(fs::read_dir(chosen)?.count(), 0);
	assert_eq!(fs::read_dir(moved)?.count(), 0);
	Ok(())
}
