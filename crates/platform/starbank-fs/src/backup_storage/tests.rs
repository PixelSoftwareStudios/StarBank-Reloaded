use std::{error::Error, fs};

use starbank_backup::{BackupIntegrity, BackupPlacement, BackupReason, BackupRequest};

use crate::backup_source;

use super::*;

#[test]
fn sidecar_cannot_publish_into_a_replaced_managed_directory() -> Result<(), Box<dyn Error>> {
	let sandbox = tempfile::tempdir()?;
	let root = sandbox.path().canonicalize()?;
	let chosen = root.join("Chosen");
	let moved = root.join("Original");
	fs::create_dir(&chosen)?;
	let grant = BackupDirectory::capture(&chosen)?;
	let source = backup_source(&root.join("Fixture.SC2Bank"))?;
	let request = BackupRequest {
		timestamp: "20261127_120000".parse()?,
		placement: BackupPlacement::Managed(grant.path()),
	};
	let bytes = b"<Bank/>";
	let record = BackupRecord::from_metadata(
		source,
		request,
		0,
		BackupReason::Requested,
		BackupIntegrity::measure(bytes),
	)?;
	let path = record.id().path();
	let mut storage = NativeStorage::new(Some(&grant));
	assert!(matches!(
		storage.commit(path, bytes, StorageIntent::CreateNew, 4096),
		CommitState::Verified(_)
	));

	fs::rename(&chosen, &moved)?;
	fs::create_dir(&chosen)?;
	assert!(matches!(
		storage.record_backup(&record),
		Err(RecordFailure::Rejected(StorageError::Conflict))
	));
	assert_eq!(fs::read_dir(&chosen)?.count(), 0);
	assert_eq!(
		fs::read(moved.join(path.file_name().expect("payload leaf")))?,
		bytes
	);
	assert_eq!(fs::read_dir(&moved)?.count(), 1);
	Ok(())
}
