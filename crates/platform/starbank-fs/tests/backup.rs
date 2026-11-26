#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{error::Error, fs, num::NonZeroUsize, path::Path};

use sha2::{Digest, Sha256};
use starbank_fs::{
	BackupHealth, BackupPlacement, BackupStatus, BackupVerification, NativeBackupCatalog,
	NativeBackupError, backup_source, read_snapshot,
};

#[cfg(target_os = "macos")]
use starbank_fs::{CommitOutcome, NativeBackupPolicy};

const LIMIT: usize = 4_096;
const SAVED: &[u8] = b"<?xml version='1.0'?>\r\n<Bank version='1'/>\r\n";
const CURRENT: &[u8] = b"<Bank version='2'/>";

fn catalog() -> NativeBackupCatalog {
	NativeBackupCatalog::new(NonZeroUsize::new(LIMIT).expect("test limit"))
}

#[test]
fn reads_historical_adjacent_sidecar_and_verifies_the_inferred_context()
-> Result<(), Box<dyn Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let bank_dir = root.join("Accounts/1234/1-S2-1-44/Banks/2-S2-1-22");
	fs::create_dir_all(&bank_dir)?;
	let path = bank_dir.join("Scout.SC2Bank");
	let backup = bank_dir.join("Scout_20261126_193800.sc2bank.bak");
	let sidecar = backup.with_extension("bak.json");
	fs::write(&path, CURRENT)?;
	fs::write(&backup, SAVED)?;
	let metadata = serde_json::json!({
		"schema_version": 1,
		"source_name": "Scout.SC2Bank",
		"signing_context": {
			"author": "2-S2-1-22",
			"player": "1-S2-1-44",
			"bank_name": "Scout",
		},
		"timestamp_utc": "20261126_193800",
		"sequence": 0,
		"reason": "explicit_save",
		"byte_length": SAVED.len(),
		"sha256": hex::encode(Sha256::digest(SAVED)),
	});
	fs::write(&sidecar, serde_json::to_vec(&metadata)?)?;
	let source = backup_source(&path)?;
	let mut catalog = catalog();
	assert_eq!(
		catalog.load(
			&source,
			BackupPlacement::Adjacent,
			BackupVerification::Verify
		)?,
		0
	);
	assert_eq!(
		catalog.status(&source),
		BackupStatus::Latest(BackupHealth::Verified)
	);
	let prepared = catalog.prepare_restore(
		&source,
		read_snapshot(&path, LIMIT)?,
		&backup,
		BackupPlacement::Adjacent,
	)?;
	assert_eq!(prepared.current().bytes(), CURRENT);
	assert_eq!(prepared.selected().bytes(), SAVED);
	assert_eq!(prepared.record().id().path(), backup);

	let mut relocated = metadata;
	relocated["signing_context"]["player"] = "1-S2-1-43".into();
	fs::write(&sidecar, serde_json::to_vec(&relocated)?)?;
	assert!(matches!(
		catalog.prepare_restore(
			&source,
			read_snapshot(&path, LIMIT)?,
			&backup,
			BackupPlacement::Adjacent
		),
		Err(NativeBackupError::MetadataMismatch)
	));
	assert_eq!(fs::read(&path)?, CURRENT);
	Ok(())
}

#[test]
fn shared_managed_directory_distinguishes_two_sources_with_the_same_name()
-> Result<(), Box<dyn Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let managed = root.join("catalog");
	let first_dir = root.join("one");
	let second_dir = root.join("two");
	for directory in [&managed, &first_dir, &second_dir] {
		fs::create_dir(directory)?;
	}
	let first_path = first_dir.join("Scout.SC2Bank");
	let second_path = second_dir.join("Scout.SC2Bank");
	fs::write(&first_path, SAVED)?;
	fs::write(&second_path, CURRENT)?;
	let first = backup_source(&first_path)?;
	let second = backup_source(&second_path)?;
	let location = BackupPlacement::Managed(&managed);
	let mut originals = catalog();
	let one = originals.create(&first, location)?;
	let two = originals.create(&second, location)?;
	assert_ne!(one, two);
	assert_eq!(fs::read(one.path())?, SAVED);
	assert_eq!(fs::read(two.path())?, CURRENT);

	let mut reopened = catalog();
	assert_eq!(
		reopened.load(&first, location, BackupVerification::Verify)?,
		0
	);
	assert_eq!(reopened.history(&first).len(), 1);
	assert_eq!(reopened.history(&first)[0].0.id(), &one);
	let refused = reopened.prepare_restore(
		&second,
		read_snapshot(&second_path, LIMIT)?,
		one.path(),
		location,
	);
	assert!(matches!(refused, Err(NativeBackupError::MetadataMismatch)));
	assert_eq!(
		reopened.load(&second, location, BackupVerification::Verify)?,
		0
	);
	assert_eq!(reopened.history(&second).len(), 1);
	assert_eq!(reopened.history(&second)[0].0.id(), &two);
	Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn native_catalog_keeps_first_session_backup_and_restores_exact_bytes() -> Result<(), Box<dyn Error>>
{
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let path = root.join("Scout.SC2Bank");
	let managed = root.join("catalog");
	fs::create_dir(&managed)?;
	fs::write(&path, SAVED)?;
	let source = backup_source(&path)?;
	let location = BackupPlacement::Managed(&managed);
	let policy = NativeBackupPolicy::FirstInSession(location);
	let mut catalog = catalog();
	let at_open = read_snapshot(&path, LIMIT)?;
	let first = catalog.overwrite(&source, at_open.fingerprint(), CURRENT, policy)?;
	let saved_backup = first.backup.expect("first overwrite kept a backup");
	let CommitOutcome::CommittedVerified(receipt) = first.commit else {
		panic!("first overwrite did not commit");
	};
	assert_eq!(fs::read(saved_backup.path())?, SAVED);
	assert_eq!(fs::read(&path)?, CURRENT);

	let subsequent = catalog.overwrite(&source, receipt.fingerprint(), b"<Bank/>", policy)?;
	assert!(matches!(
		subsequent.commit,
		CommitOutcome::CommittedVerified(_)
	));
	assert!(subsequent.backup.is_none());
	let explicit = catalog.overwrite(
		&source,
		read_snapshot(&path, LIMIT)?.fingerprint(),
		CURRENT,
		NativeBackupPolicy::Explicit,
	)?;
	assert!(matches!(
		explicit.commit,
		CommitOutcome::CommittedVerified(_)
	));
	assert_eq!(
		explicit.backup.expect("explicit backup").path().parent(),
		Some(root.as_path())
	);

	let restore = catalog.prepare_restore(
		&source,
		read_snapshot(&path, LIMIT)?,
		saved_backup.path(),
		location,
	)?;
	let restored = catalog.restore(restore, location)?;
	assert!(matches!(
		restored.commit,
		CommitOutcome::CommittedVerified(_)
	));
	assert_eq!(
		fs::read(restored.backup.expect("reversal backup").path())?,
		CURRENT
	);
	assert_eq!(fs::read(saved_backup.path())?, SAVED);
	assert_eq!(fs::read(&path)?, SAVED);
	Ok(())
}

#[test]
fn unsafe_managed_locations_are_refused_before_loading() -> Result<(), Box<dyn Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let source = backup_source(&root.join("Scout.SC2Bank"))?;
	let verification = BackupVerification::Recorded;
	let mut catalog = catalog();
	assert!(matches!(
		catalog.load(
			&source,
			BackupPlacement::Managed(Path::new("relative")),
			verification
		),
		Err(NativeBackupError::InvalidLocation)
	));
	assert_eq!(
		catalog.load(
			&source,
			BackupPlacement::Managed(&root.join("missing")),
			verification
		)?,
		0
	);
	Ok(())
}
