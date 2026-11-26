use std::{fs, io};

use starbank_editor::PROTOCOL_VERSION;

use super::*;

const LIVE: &[u8] = b"<Bank version='1'/>";

#[test]
fn review_budget_rejects_excess_records_bytes_and_overflow() {
	assert!(check_budget(std::iter::empty()).is_ok());
	assert!(check_budget([MAX_REVIEW_PAYLOAD_BYTES].into_iter()).is_ok());
	assert_eq!(
		check_budget(std::iter::repeat_n(1, MAX_REVIEW_RECORDS + 1)),
		Err(RetentionReviewError::ReviewTooLarge)
	);
	assert_eq!(
		check_budget([MAX_REVIEW_PAYLOAD_BYTES, 1].into_iter()),
		Err(RetentionReviewError::ReviewTooLarge)
	);
	assert_eq!(
		check_budget([1, usize::MAX].into_iter()),
		Err(RetentionReviewError::ReviewTooLarge)
	);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn reviews_only_older_verified_managed_backups_and_preserves_every_file() -> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let directory = fixture.path().canonicalize()?;
	let bank = directory.join("Fixture.SC2Bank");
	let managed = directory.join("managed");
	fs::write(&bank, LIVE)?;
	fs::create_dir(&managed)?;
	let source = backup_source(&bank).expect("bank source");
	let mut writer = fresh_catalog().expect("writer");
	let placement = BackupPlacement::Managed(&managed);
	let older = writer.create(&source, placement).expect("older");
	let newest = writer.create(&source, placement).expect("newest");
	let missing = writer.create(&source, placement).expect("missing");
	let modified = writer.create(&source, placement).expect("modified");
	let adjacent = writer
		.create(&source, BackupPlacement::Adjacent)
		.expect("adjacent");
	fs::remove_file(missing.path())?;
	fs::write(modified.path(), b"payload exceeding the catalog length")?;
	fs::write(
		managed.join("Fixture_20261127_091122_954.sc2bank.bak.json"),
		b"invalid",
	)?;

	let mut catalog = fresh_catalog().expect("review catalog");
	assert!(matches!(
		review_catalog(&mut catalog, &source, &directory, NonZeroUsize::MIN),
		Err(RetentionReviewError::SameFolder)
	));
	let review =
		review_catalog(&mut catalog, &source, &managed, NonZeroUsize::MIN).expect("review");
	assert_eq!(review.keep_newest, 1);
	assert_eq!(review.verified_count, 2);
	assert_eq!(review.retained_count, 1);
	assert_eq!(review.older_count, 1);
	assert_eq!(review.missing_count, 1);
	assert_eq!(review.unverified_count, 1);
	assert_eq!(review.rejected_metadata, 1);
	assert_eq!(
		review.older[0].name,
		older
			.path()
			.file_name()
			.expect("basename")
			.to_string_lossy()
	);
	assert_eq!(review.older[0].byte_length, LIVE.len());
	assert!(!review.older[0].timestamp_utc.is_empty());
	for path in [older.path(), newest.path(), adjacent.path(), &bank] {
		assert_eq!(fs::read(path)?, LIVE);
	}
	assert!(missing.path().with_extension("bak.json").is_file());
	assert!(modified.path().is_file());
	assert!(
		!serde_json::to_string(&review)
			.expect("IPC DTO")
			.contains(directory.to_string_lossy().as_ref())
	);
	Ok(())
}

#[test]
fn retention_review_pins_the_applied_document_revision_without_renderer_path_input()
-> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let path = fixture.path().canonicalize()?.join("Fixture.SC2Bank");
	fs::write(&path, LIVE)?;
	let engine = DesktopEngine::default();
	let open = serde_json::json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind":"open"}
	});
	let mut session = engine.engine.lock().expect("engine");
	session
		.open_path(&open.to_string(), &path)
		.expect("authorized fixture");
	let active = session
		.editor
		.native_active_target()
		.expect("open document");
	drop(session);
	let request = ActiveTarget {
		document_id: active.document_id,
		revision: active.revision.to_string(),
		request_id: "review-retention".into(),
	};
	assert_eq!(
		pin_selection(&engine, &request)
			.expect("current target")
			.path,
		path
	);
	let stale = ActiveTarget {
		revision: "18446744073709551614".into(),
		..request
	};
	assert_eq!(
		pin_selection(&engine, &stale),
		Err(RetentionReviewError::StaleBank)
	);
	assert!(
		serde_json::from_str::<ActiveTarget>(
			r#"{"documentId":"d","revision":"0","requestId":"review","path":"/elsewhere"}"#
		)
		.is_err()
	);
	Ok(())
}
