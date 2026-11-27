use std::num::NonZeroUsize;

use starbank_editor::{ExportSigning, NativeRestorePolicy};
use starbank_fs::{
	BackupPlacement, CommitIntent, CommitOutcome, CommitReceipt, NativeBackupCatalog,
	NativeBackupPolicy, read_snapshot,
};

use super::*;

#[test]
fn verified_write_with_rejected_acknowledgement_retains_evidence_and_the_actual_dirty_engine()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, destination) = dirty_session()?;
	let (_foreign_folder, foreign, _) = dirty_session()?;
	let before = session.editor.native_active_target().expect("dirty target");
	let foreign_target = foreign
		.editor
		.native_active_target()
		.expect("foreign target");
	let prepared = foreign.editor.prepare_native_active_save(
		&foreign_target.document_id,
		foreign_target.revision,
		ExportSigning::Preserve,
	)?;
	let (committed, backup) = overwrite(&destination, prepared.bytes())?;
	let displaced = committed
		.recovery()
		.expect("APFS displaced artifact")
		.path()
		.to_owned();
	let acknowledged = session
		.editor
		.acknowledge_verified_native_active_save(prepared);
	assert!(
		acknowledged.is_err(),
		"foreign acknowledgement must fail in the actual engine"
	);
	let failure = verified_result(
		&mut session,
		context(
			RecoveryOperation::SaveOverwrite,
			&destination,
			backup.as_ref(),
		),
		&committed,
		acknowledged,
	)
	.expect_err("editor acknowledgement failed");
	let report = encoded(failure);
	assert_verified(
		&report,
		"save_overwrite",
		true,
		&destination,
		Some(&displaced),
	);
	assert_eq!(
		report["backup"]["path"]["display"],
		backup
			.as_ref()
			.expect("verified backup")
			.path()
			.to_string_lossy()
			.as_ref()
	);
	assert_scope(&report, &before);
	assert_eq!(
		session.editor.native_active_target().expect("same target"),
		before
	);
	assert!(session.selected.is_none());
	assert_eq!(fs::read(&destination)?, EDITED);
	assert_eq!(fs::read(displaced)?, ORIGINAL);
	assert_eq!(fs::read(backup.expect("backup").path())?, ORIGINAL);
	Ok(())
}

#[test]
fn verified_save_as_then_snapshot_failure_preserves_a_clean_no_op_revision_and_physical_scope()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, original) = dirty_session()?;
	let before = session.editor.native_active_target().expect("dirty target");
	let prepared = session.editor.prepare_native_active_save(
		&before.document_id,
		before.revision,
		ExportSigning::Preserve,
	)?;
	let destination = original.with_file_name("Saved Copy.SC2Bank");
	let committed = verified_commit(&destination, prepared.bytes(), CommitIntent::CreateNew);
	let acknowledged = session
		.editor
		.acknowledge_verified_native_active_save(prepared);
	let revision = verified_result(
		&mut session,
		context(RecoveryOperation::SaveAs, &destination, None),
		&committed,
		acknowledged,
	)
	.map_err(native_error)?;
	assert_eq!(
		revision, before.revision,
		"unsigned acknowledgement does not invent a revision"
	);
	let accepted = session
		.editor
		.native_active_target()
		.expect("acknowledged target");
	assert!(!accepted.dirty);
	session.selected = Some(crate::save::SelectedBank {
		document_id: accepted.document_id.clone(),
		path: destination.clone(),
		fingerprint: Some(committed.fingerprint().clone()),
	});
	let snapshot = session.snapshot("", accepted.document_id.clone(), revision, &destination);
	assert!(
		snapshot.is_err(),
		"the real snapshot rejects an invalid request ID"
	);
	let report = encoded(
		verified_result(
			&mut session,
			context(RecoveryOperation::SaveAs, &destination, None),
			&committed,
			snapshot,
		)
		.err()
		.expect("snapshot rejected"),
	);
	assert_verified(&report, "save_as", false, &destination, None);
	assert_eq!(report["backup"], json!({"kind":"not_created"}));
	assert_scope(&report, &accepted);
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("accepted state retained"),
		accepted
	);
	assert!(session.selected.is_none());
	assert_eq!(fs::read(original)?, ORIGINAL);
	assert_eq!(fs::read(destination)?, EDITED);
	Ok(())
}

#[test]
fn verified_signed_save_then_snapshot_failure_keeps_the_acknowledged_revision()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, destination) = dirty_session()?;
	let before = session.editor.native_active_target().expect("dirty target");
	let signing = starbank_signature::SigningContext::new("2-S2-1-22", "1-S2-1-44", "Current")?;
	let prepared = session.editor.prepare_native_active_save(
		&before.document_id,
		before.revision,
		ExportSigning::Sign(&signing),
	)?;
	let bytes = prepared.bytes().to_vec();
	let (committed, backup) = overwrite(&destination, &bytes)?;
	let acknowledged = session
		.editor
		.acknowledge_verified_native_active_save(prepared);
	let revision = verified_result(
		&mut session,
		context(
			RecoveryOperation::SaveOverwrite,
			&destination,
			backup.as_ref(),
		),
		&committed,
		acknowledged,
	)
	.map_err(native_error)?;
	assert!(
		revision > before.revision,
		"installing the signature changes the model revision"
	);
	let accepted = session
		.editor
		.native_active_target()
		.expect("signed target");
	assert!(!accepted.dirty);
	let snapshot = session.snapshot("", accepted.document_id.clone(), revision, &destination);
	let report = encoded(
		verified_result(
			&mut session,
			context(
				RecoveryOperation::SaveOverwrite,
				&destination,
				backup.as_ref(),
			),
			&committed,
			snapshot,
		)
		.err()
		.expect("snapshot rejected"),
	);
	assert_verified(
		&report,
		"save_overwrite",
		true,
		&destination,
		committed.recovery().map(|artifact| artifact.path()),
	);
	assert_scope(&report, &accepted);
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("acknowledged state"),
		accepted
	);
	assert_eq!(fs::read(destination)?, bytes);
	assert!(session.selected.is_none());
	Ok(())
}

#[test]
fn restored_new_document_scope_survives_snapshot_failure_with_its_reversal_reference()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, destination) = dirty_session()?;
	let before = session.editor.native_active_target().expect("dirty target");
	let source = backup_source(&destination)?;
	let mut catalog = NativeBackupCatalog::new(
		NonZeroUsize::new(max_attachment_bytes()).expect("positive limit"),
	);
	let original_backup = catalog.create(&source, BackupPlacement::Adjacent)?;
	let current = read_snapshot(&destination, max_attachment_bytes())?;
	let storage = catalog.prepare_restore(
		&source,
		current,
		original_backup.path(),
		BackupPlacement::Adjacent,
	)?;
	let prepared = session.editor.prepare_native_active_restore(
		&before.document_id,
		before.revision,
		storage.selected().bytes(),
		NativeRestorePolicy::DiscardChanges,
	)?;
	let effect = catalog.restore(storage, BackupPlacement::Adjacent)?;
	let committed = take_verified(effect.commit);
	let acknowledged = session
		.editor
		.acknowledge_verified_native_active_restore(prepared);
	let restored = verified_result(
		&mut session,
		context(
			RecoveryOperation::Restore,
			&destination,
			effect.backup.as_ref(),
		),
		&committed,
		acknowledged,
	)
	.map_err(native_error)?;
	let accepted = session
		.editor
		.native_active_target()
		.expect("restored target");
	assert_ne!(accepted.document_id, before.document_id);
	assert_eq!(accepted.document_id, restored.document_id);
	assert_eq!(accepted.revision, restored.revision);
	assert!(!accepted.dirty);
	let snapshot = session.snapshot(
		"restore-snapshot",
		before.document_id,
		before.revision,
		&destination,
	);
	let report = encoded(
		verified_result(
			&mut session,
			context(
				RecoveryOperation::Restore,
				&destination,
				effect.backup.as_ref(),
			),
			&committed,
			snapshot,
		)
		.err()
		.expect("retired scope rejected"),
	);
	assert_verified(
		&report,
		"restore",
		true,
		&destination,
		committed.recovery().map(|artifact| artifact.path()),
	);
	assert_scope(&report, &accepted);
	let reversal = effect.backup.expect("verified reversal");
	assert_eq!(
		report["backup"]["path"]["display"],
		reversal.path().to_string_lossy().as_ref()
	);
	assert_eq!(fs::read(reversal.path())?, ORIGINAL);
	assert_eq!(fs::read(original_backup.path())?, ORIGINAL);
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("restored state retained"),
		accepted
	);
	assert!(session.selected.is_none());
	Ok(())
}

#[test]
fn all_post_verified_snapshot_error_variants_keep_the_verified_classification()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, original) = dirty_session()?;
	let destination = original.with_file_name("Copy.SC2Bank");
	let committed = verified_commit(&destination, EDITED, CommitIntent::CreateNew);
	for error in [
		CommandError::SavedNeedsReopen,
		CommandError::InvalidPacket,
		CommandError::File(crate::selection::OpenError::InvalidName),
	] {
		let report = encoded(
			verified_result::<(), _>(
				&mut session,
				context(RecoveryOperation::SaveAs, &destination, None),
				&committed,
				Err(error),
			)
			.expect_err("post-write error"),
		);
		assert_verified(&report, "save_as", false, &destination, None);
	}
	assert_eq!(fs::read(destination)?, EDITED);
	Ok(())
}

#[test]
fn unavailable_engine_scope_is_omitted_instead_of_invented() -> Result<(), Box<dyn Error>> {
	let folder = tempfile::tempdir()?;
	let destination = folder.path().canonicalize()?.join("Copy.SC2Bank");
	let committed = verified_commit(&destination, ORIGINAL, CommitIntent::CreateNew);
	let mut session = DesktopSession::default();
	let report = encoded(
		verified_result::<(), _>(
			&mut session,
			context(RecoveryOperation::SaveAs, &destination, None),
			&committed,
			Err(CommandError::Unavailable),
		)
		.expect_err("no editor"),
	);
	assert!(report.get("editorTarget").is_none());
	assert_verified(&report, "save_as", false, &destination, None);
	assert_eq!(fs::read(destination)?, ORIGINAL);
	Ok(())
}

fn context<'a>(
	operation: RecoveryOperation,
	destination: &'a Path,
	backup: Option<&'a BackupId>,
) -> WriteContext<'a> {
	WriteContext {
		operation,
		destination,
		backup,
	}
}

fn overwrite(
	path: &Path,
	bytes: &[u8],
) -> Result<(CommitReceipt, Option<BackupId>), Box<dyn Error>> {
	let source = backup_source(path)?;
	let expected = read_snapshot(path, max_attachment_bytes())?;
	let mut catalog = NativeBackupCatalog::new(
		NonZeroUsize::new(max_attachment_bytes()).expect("positive limit"),
	);
	let effect = catalog.overwrite(
		&source,
		expected.fingerprint(),
		bytes,
		NativeBackupPolicy::Explicit,
	)?;
	Ok((take_verified(effect.commit), effect.backup))
}

fn verified_commit(path: &Path, bytes: &[u8], intent: CommitIntent<'_>) -> CommitReceipt {
	take_verified(starbank_fs::commit(
		path,
		bytes,
		intent,
		max_attachment_bytes(),
	))
}

fn take_verified(outcome: CommitOutcome) -> CommitReceipt {
	let CommitOutcome::CommittedVerified(receipt) = outcome else {
		panic!("synthetic APFS commit must verify: {outcome:?}")
	};
	receipt
}

fn assert_verified(
	report: &Value,
	operation: &str,
	affects_active: bool,
	destination: &Path,
	artifact: Option<&Path>,
) {
	assert_eq!(report["outcome"], "verified_needs_reopen");
	assert_eq!(report["operation"], operation);
	assert_eq!(report["affectsActive"], affects_active);
	assert_eq!(
		report["destination"]["display"],
		destination.to_string_lossy().as_ref()
	);
	if let Some(artifact) = artifact {
		assert_eq!(report["recovery"]["kind"], "recorded");
		assert_eq!(
			report["recovery"]["path"]["display"],
			artifact.to_string_lossy().as_ref()
		);
	} else {
		assert_eq!(report["recovery"], json!({"kind":"unknown"}));
	}
}

fn assert_scope(report: &Value, active: &starbank_editor::NativeActiveTarget) {
	assert_eq!(
		report["editorTarget"],
		json!({"document_id":active.document_id, "expected_revision":active.revision.to_string()})
	);
}

fn native_error(error: CommandError) -> std::io::Error {
	std::io::Error::other(format!("{error:?}"))
}
