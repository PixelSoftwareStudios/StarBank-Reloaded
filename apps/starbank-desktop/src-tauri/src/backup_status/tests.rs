use std::{fs, io};

use starbank_editor::PROTOCOL_VERSION;

use super::*;

const LIVE: &[u8] = b"<Bank version='1'/>";
const EDITED: &[u8] = b"<Bank version='2'/>";

#[test]
fn only_a_complete_empty_history_is_never_backed_up() -> io::Result<()> {
	let root = tempfile::tempdir()?;
	let directory = root.path().canonicalize()?;
	let bank = directory.join("Fixture.SC2Bank");
	let managed = directory.join("managed");
	fs::write(&bank, LIVE)?;

	let empty = recorded_status(&bank, Some(&managed)).expect("empty catalog");
	assert_eq!(empty.state, Status::NeverBackedUp);
	assert!(!empty.incomplete);
	assert_eq!(empty.record_count, 0);
	assert!(!managed.exists(), "status must not create its directory");

	let unavailable = recorded_status(&bank, None).expect("unavailable managed location");
	assert_eq!(unavailable.state, Status::Unknown);
	assert!(unavailable.incomplete);

	fs::write(
		directory.join("Fixture_20261126_225000.sc2bank.bak.json"),
		b"malformed",
	)?;
	let suspect = recorded_status(&bank, Some(&managed)).expect("malformed catalog");
	assert_eq!(suspect.state, Status::Unknown);
	assert!(suspect.incomplete);
	assert_eq!(suspect.rejected_metadata, 1);
	assert_eq!(suspect.latest_timestamp_utc, None);

	let status = serde_json::to_string(&suspect).expect("native status DTO");
	assert!(!status.contains(directory.to_string_lossy().as_ref()));
	Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn recorded_status_uses_current_sidecars_and_does_not_imply_payload_integrity() -> io::Result<()> {
	let root = tempfile::tempdir()?;
	let directory = root.path().canonicalize()?;
	let bank = directory.join("Fixture.SC2Bank");
	let managed = directory.join("managed");
	fs::create_dir(&managed)?;
	fs::write(&bank, LIVE)?;
	let source = backup_source(&bank).expect("portable bank");
	let mut writer = fresh_catalog().expect("new writer catalog");
	let adjacent = writer
		.create(&source, BackupPlacement::Adjacent)
		.expect("adjacent backup");
	let managed_backup = writer
		.create(&source, BackupPlacement::Managed(&managed))
		.expect("managed backup");
	fs::remove_file(adjacent.path())?;
	fs::write(managed_backup.path(), b"tampered")?;

	let status = recorded_status(&bank, Some(&managed)).expect("recorded sidecars");
	assert_eq!(status.state, Status::Recorded);
	assert_eq!(status.record_count, 2);
	assert_eq!(status.rejected_metadata, 0);
	assert!(status.latest_timestamp_utc.is_some());
	assert!(!status.incomplete);

	fs::remove_file(adjacent.path().with_extension("bak.json"))?;
	fs::remove_file(managed_backup.path().with_extension("bak.json"))?;
	let status = recorded_status(&bank, Some(&managed)).expect("current sidecars removed");
	assert_eq!(status.state, Status::NeverBackedUp);
	assert_eq!(status.record_count, 0);
	assert_eq!(fs::read(&bank)?, LIVE);
	Ok(())
}

#[test]
fn renderer_must_target_the_exact_open_document_and_revision() -> io::Result<()> {
	let root = tempfile::tempdir()?;
	let bank = root.path().canonicalize()?.join("Fixture.SC2Bank");
	fs::write(&bank, LIVE)?;
	let engine = DesktopEngine::default();
	{
		let mut session = engine.engine.lock().expect("engine");
		let open = serde_json::json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "open",
			"command": {"kind": "open"}
		});
		session
			.open_path(&open.to_string(), &bank)
			.expect("open native selection");
	}
	let active = engine
		.engine
		.lock()
		.expect("engine")
		.editor
		.native_active_target()
		.expect("target");
	let request = ActiveTarget {
		request_id: String::from("status"),
		document_id: active.document_id.clone(),
		revision: active.revision.to_string(),
	};
	assert_eq!(
		selected(&engine, &request).expect("pinned selection").path,
		bank
	);
	let forged = ActiveTarget {
		request_id: String::from("status"),
		document_id: String::from("different-document"),
		revision: request.revision.clone(),
	};
	assert!(selected(&engine, &forged).is_err());
	let edit = serde_json::json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "edit",
		"document_id": active.document_id,
		"expected_revision": active.revision.to_string(),
		"command": {"kind": "replace_raw"}
	});
	engine
		.engine
		.lock()
		.expect("engine")
		.dispatch(&edit.to_string(), Some(EDITED));
	assert!(selected(&engine, &request).is_err());
	let forged_path = r#"{"requestId":"status","documentId":"x","revision":"1","path":"/outside"}"#;
	assert!(serde_json::from_str::<ActiveTarget>(forged_path).is_err());
	Ok(())
}
