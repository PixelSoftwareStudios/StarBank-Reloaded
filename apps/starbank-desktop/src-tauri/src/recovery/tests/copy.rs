use super::*;

#[test]
fn copy_family_receipts_preserve_current_editor_authority_and_record_no_preparatory_backup()
-> Result<(), Box<dyn Error>> {
	let (folder, session, original) = dirty_session()?;
	let before = session.editor.native_active_target().expect("dirty target");
	let fingerprint = session
		.selected
		.as_ref()
		.expect("selection")
		.fingerprint
		.clone();
	for (operation, tag) in [
		(RecoveryOperation::SaveAs, "save_as"),
		(RecoveryOperation::ExportCopy, "export_copy"),
		(RecoveryOperation::BackupCopy, "backup_copy"),
		(
			RecoveryOperation::ExternalSignedCopy,
			"external_signed_copy",
		),
	] {
		let destination = folder.path().join(format!("{tag}.SC2Bank"));
		let artifact = folder.path().join(format!(".starbank-pending-{tag}"));
		let receipt = encoded(uncertain_result(
			WriteContext {
				operation,
				destination: &destination,
				backup: None,
			},
			Some(&artifact),
			&StorageError::Conflict,
		));
		assert_eq!(receipt["operation"], tag);
		assert_eq!(receipt["outcome"], "commit_uncertain");
		assert_eq!(receipt["affectsActive"], false);
		assert_eq!(
			receipt["destination"]["display"],
			destination.to_string_lossy().as_ref()
		);
		assert_eq!(receipt["recovery"]["kind"], "recorded");
		assert_eq!(
			receipt["recovery"]["path"]["display"],
			artifact.to_string_lossy().as_ref()
		);
		assert_eq!(receipt["backup"], json!({ "kind": "not_created" }));
		assert_eq!(
			session.editor.native_active_target().expect("target"),
			before
		);
		assert_eq!(
			session.selected.as_ref().expect("selection").fingerprint,
			fingerprint
		);
		assert_eq!(fs::read(&original)?, ORIGINAL);
		assert!(
			!destination.exists(),
			"mapping must not retry or create output"
		);
		assert!(
			!artifact.exists(),
			"recorded paths are not existence claims"
		);
	}
	Ok(())
}

#[test]
fn copy_family_unknown_recovery_locations_are_not_inferred_from_the_destination() {
	for operation in [
		RecoveryOperation::ExportCopy,
		RecoveryOperation::BackupCopy,
		RecoveryOperation::ExternalSignedCopy,
	] {
		let receipt = encoded(uncertain_result(
			WriteContext {
				operation,
				destination: Path::new("/synthetic/Copy.SC2Bank"),
				backup: None,
			},
			None,
			&StorageError::Conflict,
		));
		assert_eq!(receipt["affectsActive"], false);
		assert_eq!(receipt["recovery"], json!({ "kind": "unknown" }));
		assert_eq!(receipt["backup"], json!({ "kind": "not_created" }));
	}
}
