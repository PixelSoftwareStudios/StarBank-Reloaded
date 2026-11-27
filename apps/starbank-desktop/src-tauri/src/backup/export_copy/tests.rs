#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{fs, io};

use starbank_core::Revision;
use starbank_fs::{BackupId, BackupPlacement, backup_source};

use super::*;
use crate::{
	backup::{BackupSelection, Location},
	save::DesktopSession,
};

const LIVE: &[u8] = b"<Bank version='2'/>";

fn record(path: &Path) -> (SelectedBackup, BackupId) {
	let source = backup_source(path).expect("test bank source");
	let mut catalog = fresh_catalog().expect("test catalog");
	let id = catalog
		.create(&source, BackupPlacement::Adjacent)
		.expect("adjacent backup");
	let history = catalog.history(&source);
	let record = history[0].0.clone();
	(
		SelectedBackup {
			record,
			location: Location::Adjacent,
		},
		id,
	)
}

#[test]
fn saves_exact_utf16_bytes_as_a_new_file_without_touching_live_bank() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let bank = root.join("Live.SC2Bank");
	let original = "\u{feff}<?xml version='1.0' encoding='UTF-16'?>\r\n<Bank version='1'/>\r\n"
		.encode_utf16()
		.flat_map(u16::to_le_bytes)
		.collect::<Vec<_>>();
	fs::write(&bank, &original)?;
	let (backup, id) = record(&bank);
	fs::write(&bank, LIVE)?;
	let copy = root.join("Extracted.SC2Bank");
	assert_eq!(
		save_copy(&backup, &copy).expect("export"),
		"Extracted.SC2Bank"
	);
	assert_eq!(fs::read(&copy)?, original);
	assert_eq!(fs::read(id.path())?, original);
	assert!(matches!(
		save_copy(&backup, &bank),
		Err(CommandError::WriteRejected(_))
	));
	assert_eq!(fs::read(bank)?, LIVE);
	Ok(())
}

#[test]
fn replacement_of_both_payload_and_sidecar_requires_a_new_review() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let bank = root.join("Live.SC2Bank");
	fs::write(&bank, b"<Bank version='1'/>")?;
	let (backup, id) = record(&bank);
	fs::write(&bank, LIVE)?;
	let (_, newer) = record(&bank);
	let sidecar = id.path().with_extension("bak.json");
	let mut previous: serde_json::Value = serde_json::from_slice(&fs::read(&sidecar)?)?;
	let changed: serde_json::Value =
		serde_json::from_slice(&fs::read(newer.path().with_extension("bak.json"))?)?;
	previous["sha256"] = changed["sha256"].clone();
	previous["byte_length"] = changed["byte_length"].clone();
	fs::write(&sidecar, serde_json::to_vec(&previous)?)?;
	fs::write(id.path(), LIVE)?;
	let copy = root.join("Extracted.SC2Bank");
	assert!(matches!(
		save_copy(&backup, &copy),
		Err(CommandError::WriteRejected(_))
	));
	assert!(!copy.exists());
	Ok(())
}

#[test]
fn malformed_backup_xml_is_refused_before_creating_a_copy() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let bank = root.join("Live.SC2Bank");
	fs::write(&bank, b"<Bank>")?;
	let (backup, _) = record(&bank);
	let copy = root.join("Extracted.SC2Bank");
	assert!(matches!(
		save_copy(&backup, &copy),
		Err(CommandError::WriteRejected(_))
	));
	assert!(!copy.exists());
	Ok(())
}

#[test]
fn backup_selection_is_one_shot_and_specific_to_the_list_request() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let bank = folder.path().canonicalize()?.join("Live.SC2Bank");
	fs::write(&bank, LIVE)?;
	let (backup, _) = record(&bank);
	let revision = Revision::INITIAL;
	let target = ActiveTarget {
		document_id: "bank".into(),
		request_id: "current-list".into(),
		revision: revision.to_string(),
	};
	let mut session = DesktopSession::default();
	session.backup_selection = Some(BackupSelection {
		request_id: target.request_id.clone(),
		document_id: target.document_id.clone(),
		revision,
		records: vec![backup],
		reviewed_restore: None,
	});
	let stale_list = ActiveTarget {
		document_id: target.document_id.clone(),
		request_id: "previous-list".into(),
		revision: revision.to_string(),
	};
	assert!(take_backup(&mut session, &stale_list, revision, 0).is_err());
	assert!(take_backup(&mut session, &target, revision, 1).is_err());
	assert!(take_backup(&mut session, &target, revision, 0).is_ok());
	assert!(take_backup(&mut session, &target, revision, 0).is_err());
	Ok(())
}
