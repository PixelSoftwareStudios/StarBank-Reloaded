use std::{fs, io};

use serde_json::json;
use starbank_fs::backup_source;

use super::*;
use crate::backup::{BackupSelection, Location, SelectedBackup};

const CURRENT: &[u8] = b"<Bank version='2'/>";
const BACKUP: &[u8] = b"<Bank version='1'/>";
const EDITED: &[u8] = b"<Bank version='3'/>";

#[test]
fn restore_review_is_read_only_and_required_for_the_selected_backup() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let (mut session, bank, target) = setup(folder.path().canonicalize()?)?;
	assert!(matches!(
		prepare_reviewed_restore(&mut session, &target, 0),
		Err(CommandError::InvalidControl)
	));
	let preview = review_backup(&mut session, &target, 0).expect("read exact reviewed bytes");
	assert_eq!(preview.comparison.first_difference_line, Some(1));
	assert_eq!(preview.comparison.before.text.as_bytes(), CURRENT);
	assert_eq!(preview.comparison.after.text.as_bytes(), BACKUP);
	assert!(preview.description.contains("discards the unsaved changes"));
	assert!(preview.description.contains("reversal backup"));
	assert!(
		session
			.editor
			.native_active_target()
			.expect("active bank")
			.dirty
	);
	assert_eq!(fs::read(&bank)?, CURRENT);

	let different_request = ActiveTarget {
		request_id: "different-list".into(),
		document_id: target.document_id.clone(),
		revision: target.revision.clone(),
	};
	assert!(matches!(
		prepare_reviewed_restore(&mut session, &different_request, 0),
		Err(CommandError::InvalidControl)
	));
	assert!(matches!(
		prepare_reviewed_restore(&mut session, &target, 1),
		Err(CommandError::InvalidControl)
	));
	let confirmed = prepare_reviewed_restore(&mut session, &target, 0).expect("prepare once");
	assert_eq!(confirmed.storage.current().bytes(), CURRENT);
	assert_eq!(confirmed.storage.selected().bytes(), BACKUP);
	assert!(session.backup_selection.is_none());
	assert_eq!(fs::read(bank)?, CURRENT);
	Ok(())
}

#[test]
fn backup_or_live_changes_after_review_reject_restore() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let (mut session, bank, target) = setup(folder.path().canonicalize()?)?;
	review_backup(&mut session, &target, 0).expect("review intact snapshot");
	let backup = session.backup_selection.as_ref().expect("list").records[0]
		.record
		.id()
		.path()
		.to_path_buf();
	fs::write(&backup, EDITED)?;
	assert!(matches!(
		prepare_reviewed_restore(&mut session, &target, 0),
		Err(CommandError::ActiveWriteRejected(_))
	));
	assert_eq!(fs::read(&bank)?, CURRENT);
	assert_eq!(fs::read(&backup)?, EDITED);
	fs::write(&backup, BACKUP)?;
	fs::write(&bank, EDITED)?;
	assert!(matches!(
		prepare_reviewed_restore(&mut session, &target, 0),
		Err(CommandError::ActiveWriteRejected(_))
	));
	assert_eq!(fs::read(backup)?, BACKUP);
	assert_eq!(fs::read(bank)?, EDITED);
	Ok(())
}

fn setup(folder: PathBuf) -> io::Result<(DesktopSession, PathBuf, ActiveTarget)> {
	let bank = folder.join("Live.SC2Bank");
	fs::write(&bank, BACKUP)?;
	let source = backup_source(&bank).expect("bank source");
	let mut catalog = fresh_catalog().expect("catalog");
	catalog
		.create(&source, BackupPlacement::Adjacent)
		.expect("backup fixture");
	let record = catalog.history(&source)[0].0.clone();
	fs::write(&bank, CURRENT)?;
	let mut session = DesktopSession::default();
	session
		.open_path(
			r#"{"schema_version":1,"request_id":"open","command":{"kind":"open"}}"#,
			&bank,
		)
		.expect("open fixture");
	let opening = session.editor.native_active_target().expect("opening");
	session.dispatch(
		&json!({
			"schema_version": 1,
			"request_id": "edit",
			"document_id": opening.document_id,
			"expected_revision": opening.revision.to_string(),
			"command": { "kind": "replace_raw" },
		})
		.to_string(),
		Some(EDITED),
	);
	let active = session.editor.native_active_target().expect("edited bank");
	let target = ActiveTarget {
		request_id: "list-and-review".into(),
		document_id: active.document_id,
		revision: active.revision.to_string(),
	};
	session.backup_selection = Some(BackupSelection {
		request_id: target.request_id.clone(),
		document_id: target.document_id.clone(),
		revision: active.revision,
		records: vec![SelectedBackup {
			record,
			location: Location::Adjacent,
		}],
		reviewed_restore: None,
	});
	Ok((session, bank, target))
}
