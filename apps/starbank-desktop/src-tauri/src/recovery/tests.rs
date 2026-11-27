use std::{error::Error, fs, path::PathBuf};

use serde_json::{Value, json};
use starbank_backup::{BackupIntegrity, BackupReason, BackupRecord, BackupRequest};
use starbank_editor::max_attachment_bytes;
use starbank_fs::{StorageError, backup_source};

use super::*;

mod copy;

const ORIGINAL: &[u8] = br#"<Bank><Section name="Original"/></Bank>"#;
const EDITED: &[u8] = br#"<Bank><Section name="Edited"/></Bank>"#;
const OPEN: &str = r#"{"schema_version":1,"request_id":"open","command":{"kind":"open"}}"#;

#[test]
fn uncertain_overwrite_and_restore_preserve_editor_state_and_report_every_recovery_reference()
-> Result<(), Box<dyn Error>> {
	for (operation, expected) in [
		(RecoveryOperation::SaveOverwrite, "save_overwrite"),
		(RecoveryOperation::Restore, "restore"),
	] {
		let (folder, mut session, destination) = dirty_session()?;
		let before = session
			.editor
			.native_active_target()
			.expect("active dirty target");
		let backup = record(&destination)?;
		let artifact = folder.path().join(".starbank-pending-synthetic");
		let error = uncertain_write(
			&mut session,
			WriteContext {
				operation,
				destination: &destination,
				backup: Some(backup.id()),
			},
			Some(&artifact),
			&StorageError::Conflict,
		);
		let receipt = encoded(error);
		assert_eq!(receipt["operation"], expected);
		assert_eq!(receipt["outcome"], "commit_uncertain");
		assert_eq!(receipt["affectsActive"], true);
		assert_eq!(
			receipt["destination"]["display"],
			destination.to_string_lossy().as_ref()
		);
		assert_eq!(receipt["recovery"]["kind"], "recorded");
		assert_eq!(
			receipt["recovery"]["path"]["display"],
			artifact.to_string_lossy().as_ref()
		);
		assert_eq!(receipt["backup"]["kind"], "verified");
		assert_eq!(
			receipt["backup"]["path"]["display"],
			backup.id().path().to_string_lossy().as_ref()
		);
		let after = session
			.editor
			.native_active_target()
			.expect("retained dirty target");
		assert_eq!(before, after);
		assert!(after.dirty);
		assert!(
			session
				.selected
				.as_ref()
				.expect("selection retained")
				.fingerprint
				.is_none()
		);
		assert_eq!(fs::read(&destination)?, ORIGINAL);
		assert!(
			!artifact.exists(),
			"recorded must not claim the path currently exists"
		);
	}
	Ok(())
}

#[test]
fn uncertain_save_as_does_not_invalidate_the_existing_bank_or_guess_missing_evidence()
-> Result<(), Box<dyn Error>> {
	let (folder, mut session, original) = dirty_session()?;
	let selected = session.selected.as_ref().expect("selection");
	let fingerprint = selected.fingerprint.clone();
	let before = session.editor.native_active_target().expect("dirty target");
	let destination = folder.path().join("New Copy.SC2Bank");
	let error = uncertain_write(
		&mut session,
		WriteContext {
			operation: RecoveryOperation::SaveAs,
			destination: &destination,
			backup: None,
		},
		None,
		&StorageError::Conflict,
	);
	let receipt = encoded(error);
	assert_eq!(receipt["operation"], "save_as");
	assert_eq!(receipt["affectsActive"], false);
	assert_eq!(receipt["recovery"], json!({ "kind": "unknown" }));
	assert_eq!(receipt["backup"], json!({ "kind": "not_created" }));
	assert_eq!(
		session.selected.as_ref().expect("selection").fingerprint,
		fingerprint
	);
	assert_eq!(
		session.editor.native_active_target().expect("target"),
		before
	);
	assert_eq!(fs::read(original)?, ORIGINAL);
	assert!(!destination.exists());
	Ok(())
}

#[test]
fn rejected_backup_metadata_keeps_live_authority_and_exposes_an_unverified_snapshot()
-> Result<(), Box<dyn Error>> {
	let (_folder, session, destination) = dirty_session()?;
	let before = session.editor.native_active_target().expect("dirty target");
	let fingerprint = session
		.selected
		.as_ref()
		.expect("selection")
		.fingerprint
		.clone();
	let backup = record(&destination)?;
	let backup_path = backup.id().path().to_owned();
	let error = backup_failure(
		WriteContext {
			operation: RecoveryOperation::SaveOverwrite,
			destination: &destination,
			backup: None,
		},
		NativeBackupError::from(BackupError::Metadata {
			record: Box::new(backup),
			failure: RecordFailure::Rejected(StorageError::Conflict),
		}),
	);
	let receipt = encoded(error);
	assert_eq!(receipt["outcome"], "live_unchanged");
	assert_eq!(receipt["affectsActive"], false);
	assert_eq!(receipt["recovery"], json!({ "kind": "unknown" }));
	assert_eq!(receipt["backup"]["kind"], "unverified");
	assert_eq!(
		receipt["backup"]["path"]["display"],
		backup_path.to_string_lossy().as_ref()
	);
	assert_eq!(
		session.selected.as_ref().expect("selection").fingerprint,
		fingerprint
	);
	assert_eq!(
		session.editor.native_active_target().expect("target"),
		before
	);
	assert_eq!(fs::read(destination)?, ORIGINAL);
	Ok(())
}

#[test]
fn uncertain_preparatory_backup_has_no_live_write_authority_or_verified_backup_claim() {
	let destination = Path::new("/synthetic/Current.SC2Bank");
	let backup = Path::new("/synthetic/Current_20261127_000000.sc2bank.bak");
	let artifact = Path::new("/synthetic/.starbank-pending-record");
	let receipt = encoded(incomplete_backup(
		WriteContext {
			operation: RecoveryOperation::Restore,
			destination,
			backup: None,
		},
		backup,
		Some(artifact),
		&StorageError::Conflict,
	));
	assert_eq!(receipt["outcome"], "live_unchanged");
	assert_eq!(receipt["affectsActive"], false);
	assert_eq!(receipt["recovery"]["kind"], "recorded");
	assert_eq!(
		receipt["recovery"]["path"]["display"],
		artifact.to_string_lossy().as_ref()
	);
	assert_eq!(receipt["backup"]["kind"], "unverified");
}

#[test]
fn display_evidence_is_utf8_bounded_and_never_silently_presented_as_an_exact_native_path() {
	let path = PathBuf::from(format!(
		"/synthetic/{}",
		"星".repeat(MAX_DISPLAY_PATH_BYTES)
	));
	let reason = "é".repeat(MAX_REASON_BYTES);
	let mut session = DesktopSession::default();
	let receipt = encoded(uncertain_write(
		&mut session,
		WriteContext {
			operation: RecoveryOperation::SaveAs,
			destination: &path,
			backup: None,
		},
		Some(&path),
		&reason,
	));
	for value in [&receipt["destination"], &receipt["recovery"]["path"]] {
		assert_eq!(value["truncated"], true);
		assert_eq!(value["lossy"], false);
		assert!(value["display"].as_str().expect("display").len() <= MAX_DISPLAY_PATH_BYTES);
	}
	assert_eq!(receipt["reasonTruncated"], true);
	assert!(receipt["reason"].as_str().expect("reason").len() <= MAX_REASON_BYTES);
}

#[cfg(unix)]
#[test]
fn non_utf8_evidence_is_marked_lossy() {
	use std::{ffi::OsString, os::unix::ffi::OsStringExt};
	let path = PathBuf::from(OsString::from_vec(b"/synthetic/\xff.SC2Bank".to_vec()));
	let encoded = serde_json::to_value(DisplayPath::new(&path)).expect("display serialization");
	assert_eq!(encoded["lossy"], true);
	assert_eq!(encoded["truncated"], false);
	assert!(
		encoded["display"]
			.as_str()
			.expect("display")
			.contains('\u{fffd}')
	);
}

#[test]
fn ordinary_backup_failures_preserve_the_existing_scope_specific_error_mapping() {
	for operation in [RecoveryOperation::SaveOverwrite, RecoveryOperation::Restore] {
		let error = backup_failure(
			WriteContext {
				operation,
				destination: Path::new("synthetic.SC2Bank"),
				backup: None,
			},
			NativeBackupError::from(BackupError::MissingFile),
		);
		match operation {
			RecoveryOperation::SaveOverwrite => {
				assert!(matches!(error, CommandError::ExternalBankChanged))
			}
			RecoveryOperation::Restore => {
				assert!(matches!(error, CommandError::ActiveWriteRejected(_)))
			}
			RecoveryOperation::SaveAs
			| RecoveryOperation::ExportCopy
			| RecoveryOperation::BackupCopy
			| RecoveryOperation::ExternalSignedCopy => unreachable!("fixture excludes copy operations"),
		}
	}
}

fn encoded(error: CommandError) -> Value {
	let value = serde_json::to_value(error).expect("native error serializes");
	assert_eq!(value.as_object().expect("tagged error").len(), 1);
	value["recovery_required"].clone()
}

fn record(path: &Path) -> Result<BackupRecord, Box<dyn Error>> {
	Ok(BackupRecord::from_metadata(
		backup_source(path)?,
		BackupRequest::adjacent("20261127_000000".parse()?),
		0,
		BackupReason::BeforeRestore,
		BackupIntegrity::measure(ORIGINAL),
	)?)
}

fn dirty_session() -> Result<(tempfile::TempDir, DesktopSession, PathBuf), Box<dyn Error>> {
	let folder = tempfile::tempdir()?;
	let path = folder.path().canonicalize()?.join("Current.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	let mut session = DesktopSession::default();
	session
		.open_path(OPEN, &path)
		.expect("synthetic bank opens");
	let target = session.editor.native_active_target().expect("open target");
	let request = json!({
		"schema_version":1,
		"request_id":"edit",
		"document_id":target.document_id,
		"expected_revision":target.revision.to_string(),
		"command":{"kind":"replace_raw_text"}
	});
	let reply = session.dispatch(&request.to_string(), Some(EDITED));
	let control: Value = serde_json::from_str(&reply.json)?;
	assert_eq!(control["result"]["kind"], "snapshot");
	assert!(
		session
			.editor
			.native_active_target()
			.expect("dirty target")
			.dirty
	);
	assert!(starbank_fs::read_snapshot(&path, max_attachment_bytes()).is_ok());
	Ok((folder, session, path))
}

#[test]
fn save_as_recovery_error_matches_the_shared_wire_fixture() {
	let mut session = DesktopSession::default();
	let error = uncertain_write(
		&mut session,
		WriteContext {
			operation: RecoveryOperation::SaveAs,
			destination: Path::new("/fixture/Copy.SC2Bank"),
			backup: None,
		},
		Some(Path::new("/fixture/.stage")),
		&"Synthetic publication could not be verified.",
	);
	let expected: Value = serde_json::from_str(include_str!("fixtures/save-as-uncertain.json"))
		.expect("decode shared recovery fixture");
	assert_eq!(
		serde_json::to_value(error).expect("serialize native command error"),
		expected
	);
}
