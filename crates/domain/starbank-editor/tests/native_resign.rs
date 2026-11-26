use serde_json::{Value, json};
use starbank_core::Revision;
use starbank_editor::{EditorError, Engine, ExportSigning, PROTOCOL_VERSION};
use starbank_signature::{SignatureStatus, SigningContext};

const ORIGINAL: &[u8] =
	br#"<Bank><Section name="Score"><Key name="Points"><Value int="0042"/></Key></Section></Bank>"#;

#[test]
fn resign_is_undoable_and_keeps_the_saved_checkpoint() {
	let (mut engine, document_id) = open();
	let context =
		SigningContext::new("1-S2-1-111", "1-S2-1-222", "Stats").expect("signing context");
	assert!(matches!(
		engine.resign_native_active("unknown", Revision::INITIAL, &context),
		Err(EditorError::WrongSession)
	));
	let signed = engine
		.resign_native_active(&document_id, Revision::INITIAL, &context)
		.expect("re-sign in memory");
	assert_eq!(Some(signed), Revision::INITIAL.checked_next());
	let state = engine
		.native_active_target()
		.expect("active signed document");
	assert!(state.dirty);
	assert_eq!(
		engine
			.resign_native_active(&document_id, signed, &context)
			.expect("identical signature is a no-op"),
		signed
	);
	assert!(matches!(
		engine.resign_native_active(&document_id, Revision::INITIAL, &context),
		Err(EditorError::StaleRevision { .. })
	));

	let snapshot = command(&mut engine, &document_id, signed, "snapshot").0;
	assert_eq!(snapshot["result"]["payload"]["signature_status"], "valid");
	assert!(
		!command(&mut engine, &document_id, signed, "raw")
			.1
			.expect("signed bytes")
			.eq(ORIGINAL)
	);

	let restored = command(&mut engine, &document_id, signed, "undo").0;
	let revision = restored["revision"]
		.as_str()
		.expect("revision")
		.parse()
		.expect("canonical revision");
	assert_eq!(restored["result"]["payload"]["dirty"], false);
	assert_eq!(
		command(&mut engine, &document_id, revision, "raw")
			.1
			.expect("opening bytes"),
		ORIGINAL
	);
}

#[test]
fn external_context_is_scoped_to_checks_and_edits() {
	let (mut engine, document_id) = open();
	let intended =
		SigningContext::new("1-S2-1-111", "1-S2-1-222", "Stats").expect("manual context");
	let other = SigningContext::new("1-S2-1-111", "1-S2-1-222", "Renamed").expect("manual context");
	assert_eq!(
		engine
			.preview_native_active_signature(&document_id, Revision::INITIAL, &intended)
			.expect("external signature check"),
		SignatureStatus::Unsigned,
	);
	assert!(matches!(
		engine.preview_native_active_signature("unknown", Revision::INITIAL, &intended),
		Err(EditorError::WrongSession)
	));
	assert!(
		!engine.native_active_target().expect("active bank").dirty,
		"checking must not change the document",
	);
	assert_eq!(
		command(&mut engine, &document_id, Revision::INITIAL, "raw")
			.1
			.as_deref(),
		Some(ORIGINAL)
	);

	let signed = engine
		.resign_native_active_for_manual_context(&document_id, Revision::INITIAL, &intended)
		.expect("sign the bank in memory");
	assert_eq!(Some(signed), Revision::INITIAL.checked_next());
	assert!(matches!(
		engine.preview_native_active_signature(&document_id, Revision::INITIAL, &intended),
		Err(EditorError::StaleRevision { .. })
	));
	assert!(matches!(
		engine.resign_native_active_for_manual_context(&document_id, Revision::INITIAL, &intended),
		Err(EditorError::StaleRevision { .. })
	));
	assert_eq!(
		engine
			.preview_native_active_signature(&document_id, signed, &intended)
			.expect("check intended context"),
		SignatureStatus::Valid,
	);
	assert_eq!(
		engine
			.preview_native_active_signature(&document_id, signed, &other)
			.expect("check another context"),
		SignatureStatus::Invalid,
	);

	let snapshot = command(&mut engine, &document_id, signed, "snapshot").0;
	assert_eq!(
		snapshot["result"]["payload"]["signature_status"],
		"missing_context"
	);
	assert_eq!(snapshot["result"]["payload"]["dirty"], true);
	assert_eq!(snapshot["result"]["payload"]["can_undo"], true);
	for signing in [ExportSigning::Preserve, ExportSigning::PreserveFor(None)] {
		let prepared = engine
			.prepare_native_active_save(&document_id, signed, signing)
			.expect("prepare without a destination context");
		assert_eq!(prepared.signature_status(), SignatureStatus::MissingContext);
	}
	let undone = command(&mut engine, &document_id, signed, "undo").0;
	assert_eq!(undone["result"]["payload"]["dirty"], false);
	assert_eq!(undone["result"]["payload"]["signature_status"], "unsigned");
}

fn open() -> (Engine, String) {
	let mut engine = Engine::default();
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind": "open"},
	});
	let reply = engine.dispatch(&control.to_string(), Some(ORIGINAL));
	let reply: Value = serde_json::from_str(&reply.json).expect("open response");
	assert_eq!(reply["result"]["kind"], "snapshot");
	(
		engine,
		reply["document_id"]
			.as_str()
			.expect("document ID")
			.to_owned(),
	)
}

fn command(
	engine: &mut Engine,
	document_id: &str,
	revision: Revision,
	kind: &str,
) -> (Value, Option<Vec<u8>>) {
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "session",
		"document_id": document_id,
		"expected_revision": revision.to_string(),
		"command": {"kind": kind},
	});
	let reply = engine.dispatch(&control.to_string(), None);
	let json = serde_json::from_str(&reply.json).expect("command response");
	(json, reply.bytes)
}
