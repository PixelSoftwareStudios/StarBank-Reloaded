use serde_json::{Value, json};
use starbank_core::Revision;
use starbank_editor::{
	EditorError, Engine, ExportSigning, NativeActiveTarget, NativeRestoreError,
	NativeRestorePolicy, PROTOCOL_VERSION, WireReply, max_attachment_bytes,
};
use starbank_signature::SignatureStatus;

const ORIGINAL: &[u8] = b"<Bank/>";
const EDITED: &[u8] = br#"<Bank><Section name="Edited"/></Bank>"#;
const BACKUP: &[u8] = b"\xef\xbb\xbf<Bank>\r\n  <Section name=\"Original\"><Key name=\"Score\"><Value int=\"1,002\"/></Key></Section>\r\n</Bank>";

fn open(engine: &mut Engine, bytes: &[u8]) -> NativeActiveTarget {
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind": "open", "payload": {"discard_current": true}},
	});
	let reply = engine.dispatch(&control.to_string(), Some(bytes));
	assert_kind(&reply, "snapshot");
	engine.native_active_target().expect("active document")
}

fn command(engine: &mut Engine, kind: &str, bytes: Option<&[u8]>) -> WireReply {
	let target = engine.native_active_target().expect("active document");
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "session",
		"document_id": target.document_id,
		"expected_revision": target.revision.to_string(),
		"command": {"kind": kind},
	});
	engine.dispatch(&control.to_string(), bytes)
}

fn assert_kind(reply: &WireReply, kind: &str) -> Value {
	let response: Value = serde_json::from_str(&reply.json).expect("structured wire reply");
	assert_eq!(response["result"]["kind"], kind, "{response}");
	response
}

#[test]
fn backup_is_validated_and_adopted_only_after_verified_storage() {
	let mut engine = Engine::default();
	assert!(engine.native_active_target().is_none());
	open(&mut engine, ORIGINAL);
	assert_kind(
		&command(&mut engine, "replace_raw", Some(EDITED)),
		"snapshot",
	);
	let target = engine.native_active_target().expect("dirty document");
	assert!(target.dirty);

	assert!(matches!(
		engine.prepare_native_active_restore(
			&target.document_id,
			target.revision,
			BACKUP,
			NativeRestorePolicy::RequireClean,
		),
		Err(NativeRestoreError::DirtyDocument)
	));
	for invalid in [
		b"<Bank>".as_slice(),
		&vec![b' '; max_attachment_bytes() + 1],
	] {
		assert!(matches!(
			engine.prepare_native_active_restore(
				&target.document_id,
				target.revision,
				invalid,
				NativeRestorePolicy::DiscardChanges,
			),
			Err(NativeRestoreError::Editor(EditorError::Parse(_)))
		));
	}

	let rejected = engine
		.prepare_native_active_restore(
			&target.document_id,
			target.revision,
			BACKUP,
			NativeRestorePolicy::DiscardChanges,
		)
		.expect("backup is parseable, including its opening legacy values");
	assert_eq!(rejected.bytes(), BACKUP);
	assert_eq!(rejected.signature_status(), SignatureStatus::Unsigned);
	engine
		.validate_native_active_restore(&rejected)
		.expect("write may start");
	drop(rejected);
	assert_eq!(engine.native_active_target().as_ref(), Some(&target));
	assert_eq!(
		command(&mut engine, "raw", None).bytes.as_deref(),
		Some(EDITED)
	);

	let verified = engine
		.prepare_native_active_restore(
			&target.document_id,
			target.revision,
			BACKUP,
			NativeRestorePolicy::DiscardChanges,
		)
		.expect("backup is parseable");
	engine
		.validate_native_active_restore(&verified)
		.expect("write may start");
	let outcome = engine
		.acknowledge_verified_native_active_restore(verified)
		.expect("verified restore");
	assert_ne!(outcome.document_id, target.document_id);
	assert_eq!(outcome.revision, Revision::INITIAL);
	assert_eq!(
		engine.native_active_target(),
		Some(NativeActiveTarget {
			document_id: outcome.document_id,
			revision: outcome.revision,
			dirty: false,
		})
	);
	assert_eq!(
		command(&mut engine, "raw", None).bytes.as_deref(),
		Some(BACKUP)
	);
	let snapshot = assert_kind(&command(&mut engine, "snapshot", None), "snapshot");
	assert_eq!(snapshot["result"]["payload"]["can_undo"], false);
	assert_eq!(snapshot["result"]["payload"]["can_redo"], false);
}

#[test]
fn stale_and_foreign_restore_receipts_cannot_replace_the_current_document() {
	let mut first = Engine::default();
	let mut second = Engine::default();
	let target = open(&mut first, ORIGINAL);
	assert_eq!(target, open(&mut second, ORIGINAL));
	let receipt = first
		.prepare_native_active_restore(
			&target.document_id,
			target.revision,
			BACKUP,
			NativeRestorePolicy::DiscardChanges,
		)
		.expect("valid receipt");
	assert_eq!(receipt.document_id(), target.document_id);
	assert_eq!(receipt.revision(), target.revision);
	assert!(matches!(
		second.validate_native_active_restore(&receipt),
		Err(NativeRestoreError::Editor(EditorError::WrongSession))
	));

	assert_kind(
		&command(&mut first, "replace_raw", Some(EDITED)),
		"snapshot",
	);
	let new_revision = first
		.native_active_target()
		.expect("edited document")
		.revision;
	assert!(matches!(
		first.validate_native_active_restore(&receipt),
		Err(NativeRestoreError::Editor(EditorError::StaleRevision { expected, current }))
			if expected == target.revision && current == new_revision
	));
	assert!(matches!(
		first.acknowledge_verified_native_active_restore(receipt),
		Err(NativeRestoreError::Editor(
			EditorError::StaleRevision { .. }
		))
	));
	assert_eq!(
		command(&mut first, "raw", None).bytes.as_deref(),
		Some(EDITED)
	);

	let receipt = first
		.prepare_native_active_restore(
			&target.document_id,
			new_revision,
			BACKUP,
			NativeRestorePolicy::DiscardChanges,
		)
		.expect("valid receipt");
	let replacement = open(&mut first, ORIGINAL);
	assert!(matches!(
		first.acknowledge_verified_native_active_restore(receipt),
		Err(NativeRestoreError::Editor(EditorError::WrongSession))
	));
	assert_eq!(first.native_active_target(), Some(replacement));
}

#[test]
fn restore_rechecks_dirty_state_even_without_a_revision_change() {
	let mut engine = Engine::default();
	open(&mut engine, ORIGINAL);
	assert_kind(
		&command(&mut engine, "replace_raw", Some(EDITED)),
		"snapshot",
	);
	let previous = engine.native_active_target().expect("edited document");
	let save = engine
		.prepare_native_active_save(
			&previous.document_id,
			previous.revision,
			ExportSigning::Preserve,
		)
		.expect("valid save receipt");
	assert_kind(
		&command(&mut engine, "replace_raw", Some(ORIGINAL)),
		"snapshot",
	);
	let current = engine.native_active_target().expect("active document");
	assert!(!current.dirty);
	let restore = engine
		.prepare_native_active_restore(
			&current.document_id,
			current.revision,
			BACKUP,
			NativeRestorePolicy::RequireClean,
		)
		.expect("clean document permits restore");

	assert_eq!(
		engine
			.acknowledge_verified_native_active_save(save)
			.expect("earlier verified save"),
		current.revision
	);
	let changed = engine.native_active_target().expect("active document");
	assert_eq!(changed.revision, current.revision);
	assert!(changed.dirty);
	assert!(matches!(
		engine.validate_native_active_restore(&restore),
		Err(NativeRestoreError::DirtyDocument)
	));
	assert!(matches!(
		engine.acknowledge_verified_native_active_restore(restore),
		Err(NativeRestoreError::DirtyDocument)
	));
	assert_eq!(engine.native_active_target(), Some(changed));
	assert_eq!(
		command(&mut engine, "raw", None).bytes.as_deref(),
		Some(ORIGINAL)
	);
}
