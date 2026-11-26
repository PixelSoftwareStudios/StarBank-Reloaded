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

#[test]
fn custom_managed_directory_preserves_native_path_and_rejects_replacement()
-> Result<(), Box<dyn Error>> {
	use std::os::unix::fs::symlink;

	use starbank_fs::{BackupDirectory, RetentionPolicy, StorageError};

	let sandbox = tempfile::tempdir()?;
	let root = sandbox.path().canonicalize()?;
	#[cfg(target_os = "linux")]
	let chosen = {
		use std::os::unix::ffi::OsStrExt as _;
		root.join(std::ffi::OsStr::from_bytes(b"Choice-\xff"))
	};
	#[cfg(target_os = "macos")]
	let chosen = root.join("Choicé with spaces");
	let moved = root.join("Moved");
	let alias = root.join("Alias");
	fs::create_dir(&chosen)?;
	symlink(&chosen, &alias)?;
	assert!(BackupDirectory::capture(&alias).is_err());
	assert!(BackupDirectory::capture(&alias.join("child")).is_err());
	assert!(BackupDirectory::capture(Path::new("relative")).is_err());

	let grant = BackupDirectory::capture(&chosen)?;
	let decoded: BackupDirectory = serde_json::from_slice(&serde_json::to_vec(&grant)?)?;
	assert_eq!(grant, decoded);
	assert_eq!(grant.path(), chosen);
	decoded.validate()?;

	let bank = root.join("Scout.SC2Bank");
	fs::write(&bank, SAVED)?;
	let source = backup_source(&bank)?;
	let placement = BackupPlacement::Managed(grant.path());
	let mut catalog = catalog();
	catalog.set_managed_directory(Some(decoded));
	assert!(matches!(
		catalog.create(&source, BackupPlacement::Managed(&root)),
		Err(NativeBackupError::InvalidLocation)
	));
	let first = catalog.create(&source, placement)?;
	catalog.create(&source, placement)?;
	assert_eq!(
		catalog.load(&source, placement, BackupVerification::Recorded)?,
		0
	);
	catalog.verify(&first)?;
	assert_eq!(
		catalog.retention_candidates(&source, RetentionPolicy::KeepNewest(NonZeroUsize::MIN),)[0]
			.id(),
		&first
	);

	let mut file = fs::OpenOptions::new().write(true).open(first.path())?;
	std::io::Write::write_all(&mut file, &[b' '; SAVED.len() + 1])?;
	drop(file);
	let Err(NativeBackupError::Policy(error)) = catalog.verify(&first) else {
		panic!("a payload larger than its recorded length must fail verification");
	};
	assert!(matches!(
		*error,
		starbank_backup::BackupError::Storage(StorageError::TooLarge { max_bytes })
			if max_bytes == SAVED.len()
	));

	fs::rename(&chosen, &moved)?;
	fs::create_dir(&chosen)?;
	assert!(matches!(grant.validate(), Err(StorageError::Conflict)));
	assert!(catalog.create(&source, placement).is_err());
	assert!(
		catalog
			.load(&source, placement, BackupVerification::Verify)
			.is_err()
	);
	assert_eq!(fs::read_dir(&chosen)?.count(), 0);
	assert_eq!(fs::read_dir(&moved)?.count(), 4);
	Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn restore_keeps_confirmed_custom_source_scope_when_the_current_location_changes()
-> Result<(), Box<dyn Error>> {
	use starbank_fs::BackupDirectory;

	let sandbox = tempfile::tempdir()?;
	let root = sandbox.path().canonicalize()?;
	let path = root.join("Scout.SC2Bank");
	let old = root.join("Original backup location");
	let new = root.join("Current backup location");
	fs::create_dir(&old)?;
	fs::create_dir(&new)?;
	fs::write(&path, SAVED)?;
	let old_grant = BackupDirectory::capture(&old)?;
	let new_grant = BackupDirectory::capture(&new)?;
	let source = backup_source(&path)?;
	let old_placement = BackupPlacement::Managed(old_grant.path());
	let new_placement = BackupPlacement::Managed(new_grant.path());
	let mut catalog = catalog();
	catalog.set_managed_directory(Some(old_grant.clone()));
	let current = read_snapshot(&path, LIMIT)?;
	let first = catalog.overwrite(
		&source,
		current.fingerprint(),
		CURRENT,
		NativeBackupPolicy::FirstInSession(old_placement),
	)?;
	let backup = first.backup.expect("original custom backup");
	assert!(matches!(first.commit, CommitOutcome::CommittedVerified(_)));

	catalog.set_managed_directory(Some(new_grant.clone()));
	let current = read_snapshot(&path, LIMIT)?;
	let second = catalog.overwrite(
		&source,
		current.fingerprint(),
		CURRENT,
		NativeBackupPolicy::FirstInSession(new_placement),
	)?;
	assert!(second.backup.is_none());
	assert!(matches!(second.commit, CommitOutcome::CommittedVerified(_)));
	assert_eq!(fs::read_dir(&new)?.count(), 0);
	let prepare = |catalog: &NativeBackupCatalog| -> Result<_, Box<dyn Error>> {
		Ok(catalog.prepare_restore(
			&source,
			read_snapshot(&path, LIMIT)?,
			backup.path(),
			old_placement,
		)?)
	};
	catalog.set_managed_directory(Some(old_grant.clone()));
	let prepared = prepare(&catalog)?;
	catalog.set_managed_directory(Some(new_grant.clone()));
	let restored = catalog.restore(prepared, new_placement)?;
	assert!(matches!(
		restored.commit,
		CommitOutcome::CommittedVerified(_)
	));
	let reversal = restored.backup.expect("reversal in current location");
	assert_eq!(reversal.path().parent(), Some(new.as_path()));
	assert_eq!(fs::read(reversal.path())?, CURRENT);
	assert_eq!(fs::read(&path)?, SAVED);

	catalog.set_managed_directory(Some(old_grant.clone()));
	let pending_when_replaced = prepare(&catalog)?;
	catalog.set_managed_directory(Some(new_grant.clone()));
	let moved = root.join("Moved old location");
	fs::rename(&old, &moved)?;
	fs::create_dir(&old)?;
	// Even identical bytes at the previous path must not redirect a confirmed restore.
	let leaf = backup.path().file_name().expect("backup leaf");
	fs::copy(moved.join(leaf), old.join(leaf))?;
	let before = fs::read_dir(&new)?.count();
	assert!(
		catalog
			.restore(pending_when_replaced, new_placement)
			.is_err()
	);
	assert_eq!(fs::read_dir(&new)?.count(), before);
	assert_eq!(fs::read(&path)?, SAVED);
	Ok(())
}
