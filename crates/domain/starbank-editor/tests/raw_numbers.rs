use sc2bank::Limits;
use serde_json::{Value, json};
use starbank_core::Revision;
use starbank_editor::{
	EditorError, Engine, ExportSigning, PROTOCOL_VERSION, RawNumberKind, Session,
};
use starbank_signature::{SignatureStatus, SigningContext};

const BANK: &str = "<Bank><Section name='S'><Key name='K'><Count int='7'/><Rate fixed='1.250'/></Key></Section></Bank>";

#[test]
fn invalid_raw_numbers_leave_the_previous_bank_ready_for_signing() {
	let mut session = Session::open(BANK.as_bytes(), Limits::default(), None).unwrap();
	for (from, value, expected_kind) in [
		("int='7'", "7e5", RawNumberKind::Integer),
		("int='7'", "1,000", RawNumberKind::Integer),
		("fixed='1.250'", "NaN", RawNumberKind::Fixed),
		("fixed='1.250'", "1,000.25", RawNumberKind::Fixed),
	] {
		let (kind, _) = from.split_once('=').unwrap();
		let raw = BANK.replace(from, &format!("{kind}='{value}'"));
		assert!(matches!(
			session.replace_raw(Revision::INITIAL, raw.as_bytes()),
			Err(EditorError::InvalidRawNumber { kind, byte })
				if kind == expected_kind && Some(byte) == raw.find(value),
		));
		assert_eq!(session.revision(), Revision::INITIAL);
		assert_eq!(session.document().source(), BANK.as_bytes());
		assert!(!session.can_undo());
		assert!(!session.is_dirty());
	}

	let context = SigningContext::new("1-S2-1-100", "1-S2-1-200", "Stats").unwrap();
	let export = session
		.prepare(Revision::INITIAL, ExportSigning::Sign(&context))
		.unwrap();
	assert_eq!(export.signature_status(), SignatureStatus::Valid);
	let rendered = std::str::from_utf8(export.bytes()).unwrap();
	assert!(rendered.contains("<Count int='7'/>"));
	assert!(rendered.contains("<Rate fixed='1.250'/>"));
}

#[test]
fn valid_raw_numbers_retain_original_encoding_and_lexical_spelling() {
	let mut session = Session::open(BANK.as_bytes(), Limits::default(), None).unwrap();
	let draft = "\u{feff}<?xml version='1.0'?>\r\n<!-- numbers -->\r\n<Bank><Section name='S'><Key name='K'><Count int='9007199254740993' /><Rate fixed = '-&#x30;.000'/></Key></Section></Bank>";
	let next = session
		.replace_raw(Revision::INITIAL, draft.as_bytes())
		.unwrap();
	assert!(next > Revision::INITIAL);
	assert_eq!(session.document().source(), draft.as_bytes());
	let values = session.document().sections()[0].keys()[0].values();
	assert_eq!(values[0].attribute().unwrap().value(), "9007199254740993");
	assert_eq!(values[1].attribute().unwrap().value(), "-0.000");
	assert_eq!(
		session
			.prepare(next, ExportSigning::Preserve)
			.unwrap()
			.bytes(),
		draft.as_bytes(),
	);
	assert!(session.undo(next).unwrap() > next);
	assert_eq!(session.document().source(), BANK.as_bytes());
}

#[test]
fn legacy_invalid_occurrences_can_remain_and_cannot_multiply() {
	let legacy = "<Bank><Section name='S'><Key name='K'><Count int='1e2'/><Rate fixed='1,001.0'/></Key><Key name='K'><Count int='1e2'/></Key></Section></Bank>";
	let mut session = Session::open(legacy.as_bytes(), Limits::default(), None).unwrap();
	assert_eq!(session.document().source(), legacy.as_bytes());
	assert_eq!(
		session
			.replace_raw(Revision::INITIAL, legacy.as_bytes())
			.unwrap(),
		Revision::INITIAL,
	);
	let rebuilt = legacy.replace("<Bank>", "<Bank><!-- unrelated read-only metadata -->");
	let revision = session
		.replace_raw(Revision::INITIAL, rebuilt.as_bytes())
		.unwrap();
	assert_eq!(session.document().source(), rebuilt.as_bytes());

	let duplicates = rebuilt.replace(
		"</Section>",
		"<Key name='K'><Count int='1e2'/></Key></Section>",
	);
	assert!(matches!(
		session.replace_raw(revision, duplicates.as_bytes()),
		Err(EditorError::InvalidRawNumber {
			kind: RawNumberKind::Integer,
			..
		}),
	));
	let changed = rebuilt.replacen("1e2", "2e2", 1);
	assert!(matches!(
		session.replace_raw(revision, changed.as_bytes()),
		Err(EditorError::InvalidRawNumber {
			kind: RawNumberKind::Integer,
			..
		}),
	));
	let different_path = rebuilt.replacen("name='K'", "name='J'", 1);
	assert!(matches!(
		session.replace_raw(revision, different_path.as_bytes()),
		Err(EditorError::InvalidRawNumber { .. }),
	));
	assert_eq!(session.revision(), revision);
	assert_eq!(session.document().source(), rebuilt.as_bytes());
}

#[test]
fn unsupported_and_opaque_content_is_preserved_without_numeric_interpretation() {
	let draft = br#"<Bank><Section name='S'><Key name='K'><Unknown future='NaN'/><Group int='1e3' fixed='not numeric'/><Flag flag='legacy'/><Text string='1,234'/></Key></Section></Bank>"#;
	let mut session = Session::open(BANK.as_bytes(), Limits::default(), None).unwrap();
	assert!(session.replace_raw(Revision::INITIAL, draft).unwrap() > Revision::INITIAL);
	assert_eq!(session.document().source(), draft);
}

#[test]
fn engine_returns_raw_numeric_feedback_and_keeps_the_last_valid_snapshot() {
	let mut engine = Engine::default();
	let open = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind": "open"},
	});
	let opened = engine.dispatch(&open.to_string(), Some(BANK.as_bytes()));
	let opened: Value = serde_json::from_str(&opened.json).unwrap();
	let mut request = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "replace",
		"document_id": &opened["document_id"],
		"expected_revision": &opened["revision"],
		"command": {"kind": "replace_raw"},
	});
	let bad = BANK.replace("1.250", "1,250.500");
	let reply = engine.dispatch(&request.to_string(), Some(bad.as_bytes()));
	let result: Value = serde_json::from_str(&reply.json).unwrap();
	assert_eq!(result["result"]["kind"], "error");
	assert_eq!(result["result"]["payload"]["code"], "invalid_raw_number");
	assert!(
		result["result"]["payload"]["message"]
			.as_str()
			.unwrap()
			.contains("ungrouped"),
	);
	assert_eq!(result["revision"], opened["revision"]);
	assert!(reply.bytes.is_none());

	request["command"]["kind"] = json!("raw");
	let raw = engine.dispatch(&request.to_string(), None);
	assert_eq!(raw.bytes.as_deref(), Some(BANK.as_bytes()));
}
