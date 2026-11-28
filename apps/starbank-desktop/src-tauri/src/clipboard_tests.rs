use serde_json::{Value, json};
use starbank_editor::{Engine, PROTOCOL_VERSION};

use super::*;

const OPEN: &str = r#"{"schema_version":1,"request_id":"open","command":{"kind":"open"}}"#;
const XML: &str = "\u{feff}<Bank version=\"1\">\r\n<Section name=\"Player\"><Key name=\"Caption\"><Value string=\"星\u{a0}🎮e\u{301}\"/></Key></Section>\r\n</Bank>\r\n";

fn prepare_active(bytes: &[u8], expected_revision: Option<&str>) -> WireReply {
	let mut engine = Engine::default();
	let opened = engine.dispatch(OPEN, Some(bytes));
	let envelope: Value = serde_json::from_str(&opened.json).expect("opening envelope");
	assert_eq!(envelope["result"]["kind"], "snapshot", "{}", opened.json);

	let before = engine.native_active_target().expect("opening target");
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "copy",
		"document_id": before.document_id,
		"expected_revision": expected_revision.map_or_else(|| before.revision.to_string(), str::to_owned),
		"command": {"kind": "prepare"}
	})
	.to_string();
	validate_copy_control(&control).expect("context-free prepare");
	let reply = engine.dispatch(&control, None);
	assert_eq!(engine.native_active_target(), Some(before));
	reply
}

#[test]
fn prepare_preserves_unicode_bom_and_crlf_for_all_bank_encodings() {
	let inputs = [
		XML.as_bytes().to_vec(),
		XML.encode_utf16().flat_map(u16::to_le_bytes).collect(),
		XML.encode_utf16().flat_map(u16::to_be_bytes).collect(),
	];
	for input in inputs {
		let reply = prepare_active(&input, None);
		assert_eq!(reply.bytes.as_deref(), Some(input.as_slice()));
		assert_eq!(
			clipboard_bytes(&reply, false)
				.expect("unsigned copy")
				.as_deref(),
			Some(XML.as_bytes())
		);
	}
}

#[test]
fn engine_rejection_never_provides_clipboard_bytes() {
	let reply = prepare_active(XML.as_bytes(), Some("99"));
	let envelope: Value = serde_json::from_str(&reply.json).expect("stale reply");
	assert_eq!(envelope["result"]["payload"]["code"], "stale_revision");
	assert!(clipboard_bytes(&reply, true).expect("reply only").is_none());
}

#[test]
fn unexpected_reply_kind_or_attachment_is_rejected() {
	let prepared = prepare_active(XML.as_bytes(), None);
	let error = prepare_active(XML.as_bytes(), Some("99"));
	for reply in [
		WireReply {
			json: prepared.json,
			bytes: None,
		},
		WireReply {
			json: error.json,
			bytes: prepared.bytes,
		},
		WireReply {
			json: r#"{"result":{"kind":"snapshot"}}"#.into(),
			bytes: None,
		},
	] {
		assert!(matches!(
			clipboard_bytes(&reply, true),
			Err(CommandError::InvalidPacket)
		));
	}
}

#[test]
fn unverified_prepared_text_requires_consent() {
	let signed = XML.replace("</Bank>", "<Signature value=\"invalid\"/></Bank>");
	let reply = prepare_active(signed.as_bytes(), None);
	assert!(matches!(
		clipboard_bytes(&reply, false),
		Err(CommandError::ClipboardConsentRequired)
	));
	assert_eq!(
		clipboard_bytes(&reply, true).expect("approved").as_deref(),
		Some(signed.as_bytes())
	);
}

#[test]
fn copy_route_accepts_only_a_bounded_context_free_prepare() {
	for control in [
		OPEN,
		r#"{"command":{"kind":"prepare","payload":null}}"#,
		r#"{"command":{"kind":"prepare","payload":{"context":{"author":"different","player":"different","bank_name":"different"}}}}"#,
		r#"{"command":{"kind":"prepare","xml":"arbitrary clipboard text"}}"#,
	] {
		assert!(matches!(
			validate_copy_control(control),
			Err(CommandError::InvalidControl)
		));
	}
	assert!(matches!(
		validate_copy_control(&" ".repeat(MAX_CONTROL_BYTES + 1)),
		Err(CommandError::InvalidControl)
	));
}
