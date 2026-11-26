use serde_json::{Value, json};
use starbank_editor::{
	Engine, MAX_CONTROL_BYTES, MAX_OPERATIONS, PROTOCOL_VERSION, WireReply, max_attachment_bytes,
};

const BANK: &[u8] = br#"<?xml version="1.0"?>
<Bank version="1">
  <Section name="Stats">
    <Key name="same"><Value fixed="1.00"/><Detail string="a, b"/></Key>
    <Key name="same"><Value future="UNCHANGED"/></Key>
    <Key name=""><Value int="007"/></Key>
    <Key><Value/></Key>
    <Key name="empty"/>
  </Section>
  <Section/>
</Bank>
"#;

const CONTEXT: &str = r#"{"author":"1-S2-1-111","player":"1-S2-1-222","bank_name":"Stats"}"#;

struct Reply {
	control: Value,
	bytes: Option<Vec<u8>>,
}

impl Reply {
	fn decode(wire: WireReply) -> Self {
		Self {
			control: serde_json::from_str(&wire.json).expect("a reply must be valid JSON"),
			bytes: wire.bytes,
		}
	}

	fn document(&self) -> &str {
		self.control["document_id"].as_str().expect("document ID")
	}

	fn revision(&self) -> &str {
		self.control["revision"].as_str().expect("revision")
	}

	fn payload(&self, kind: &str) -> &Value {
		assert_eq!(self.control["result"]["kind"], kind, "{}", self.control);
		&self.control["result"]["payload"]
	}

	fn assert_error(&self, code: &str) {
		let error = self.payload("error");
		assert_eq!(error["code"], code, "{}", self.control);
		assert!(
			error["message"]
				.as_str()
				.is_some_and(|text| !text.is_empty())
		);
		assert!(self.bytes.is_none());
	}
}

fn execute(engine: &mut Engine, control: Value, bytes: Option<&[u8]>) -> Reply {
	Reply::decode(engine.dispatch(&control.to_string(), bytes))
}

fn open(engine: &mut Engine, bytes: &[u8]) -> Reply {
	execute(
		engine,
		json!({"schema_version": PROTOCOL_VERSION, "request_id": "open", "command": {"kind": "open"}}),
		Some(bytes),
	)
}

fn session_request(session: &Reply, command: Value) -> Value {
	json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "session",
		"document_id": session.document(),
		"expected_revision": session.revision(),
		"command": command,
	})
}

fn session_command(engine: &mut Engine, session: &Reply, kind: &str) -> Reply {
	execute(
		engine,
		session_request(session, json!({"kind": kind})),
		None,
	)
}

fn first_value(snapshot: &Value) -> &Value {
	&snapshot["sections"][0]["keys"][0]["values"][0]
}

#[test]
fn open_projects_distinct_occurrences_and_returns_exact_raw_bytes() {
	let mut engine = Engine::default();
	let opened = open(&mut engine, BANK);
	let snapshot = opened.payload("snapshot");
	assert_eq!(opened.control["schema_version"], PROTOCOL_VERSION);
	assert_eq!(opened.control["request_id"], "open");
	assert_eq!(opened.revision(), "0");
	assert_eq!(snapshot["dirty"], false);
	assert_eq!(snapshot["can_undo"], false);
	assert_eq!(snapshot["signature_status"], "unsigned");
	assert_eq!(first_value(snapshot)["value"], "1.00");
	assert_eq!(first_value(snapshot)["editable"], true);
	let keys = &snapshot["sections"][0]["keys"];
	assert_eq!(keys[0]["name"], keys[1]["name"]);
	assert_ne!(keys[0]["id"], keys[1]["id"]);
	assert_eq!(keys[0]["values"][1]["name"], "Detail");
	assert_eq!(keys[0]["values"][1]["value"], "a, b");
	assert_eq!(keys[1]["values"][0]["kind"], "future");
	assert_eq!(keys[1]["values"][0]["editable"], false);
	assert_eq!(keys[2]["name"], "");
	assert_eq!(keys[2]["values"][0]["value"], "007");
	assert_eq!(keys[3]["name"], Value::Null);
	assert_eq!(keys[3]["values"][0]["kind"], Value::Null);
	assert_eq!(keys[3]["values"][0]["value"], Value::Null);
	assert_eq!(keys[3]["values"][0]["editable"], false);
	assert_eq!(keys[4]["values"], json!([]));
	assert_eq!(snapshot["sections"][1]["name"], Value::Null);

	let raw = session_command(&mut engine, &opened, "raw");
	assert_eq!(raw.payload("raw"), &json!({}));
	assert_eq!(raw.bytes.as_deref(), Some(BANK));
}

#[test]
fn exact_bulk_edits_require_the_current_revision_and_support_history() {
	let mut engine = Engine::default();
	let opened = open(&mut engine, BANK);
	let snapshot = opened.payload("snapshot");
	let value = first_value(snapshot);
	let second = &snapshot["sections"][0]["keys"][0]["values"][1];
	let mutation = session_request(
		&opened,
		json!({"kind": "set_values", "payload": {"edits": [
			{"id": value["id"], "value": "9,007,199,254,740,993.50"},
			{"id": second["id"], "value": "unchanged comma, intact"},
		]}}),
	);
	let changed = execute(&mut engine, mutation.clone(), None);
	let edited = changed.payload("snapshot");
	assert_eq!(changed.revision(), "1");
	assert_eq!(edited["dirty"], true);
	assert_eq!(edited["can_undo"], true);
	assert_eq!(first_value(edited)["id"], value["id"]);
	assert_eq!(first_value(edited)["value"], "9007199254740993.50");
	assert_eq!(
		edited["sections"][0]["keys"][0]["values"][1]["value"],
		"unchanged comma, intact"
	);

	let stalled = execute(&mut engine, mutation, None);
	stalled.assert_error("stale_revision");
	assert_eq!(stalled.document(), changed.document());
	assert_eq!(stalled.revision(), changed.revision());
	let refreshed = session_command(&mut engine, &stalled, "snapshot");
	assert_eq!(refreshed.payload("snapshot"), edited);

	let undone = session_command(&mut engine, &changed, "undo");
	assert_eq!(undone.revision(), "2");
	assert_eq!(undone.payload("snapshot")["dirty"], false);
	assert_eq!(undone.payload("snapshot")["can_redo"], true);
	assert_eq!(
		session_command(&mut engine, &undone, "raw")
			.bytes
			.as_deref(),
		Some(BANK)
	);
	let redone = session_command(&mut engine, &undone, "redo");
	assert_eq!(redone.revision(), "3");
	assert_eq!(
		first_value(redone.payload("snapshot"))["value"],
		"9007199254740993.50"
	);
	let reset = session_command(&mut engine, &redone, "reset");
	assert_eq!(reset.revision(), "4");
	assert_eq!(reset.payload("snapshot")["dirty"], false);
	assert_eq!(
		session_command(&mut engine, &reset, "raw").bytes.as_deref(),
		Some(BANK)
	);
}

#[test]
fn invalid_changes_and_xml_leave_the_open_revision_intact() {
	let mut engine = Engine::default();
	let opened = open(&mut engine, BANK);
	let snapshot = opened.payload("snapshot");
	let id = &first_value(snapshot)["id"];
	let other_id = &snapshot["sections"][0]["keys"][0]["values"][1]["id"];
	let invalid = session_request(
		&opened,
		json!({"kind": "set_values", "payload": {"edits": [
			{"id": other_id, "value": "must not change"},
			{"id": id, "value": "12,34.00"},
		]}}),
	);
	let rejected = execute(&mut engine, invalid, None);
	rejected.assert_error("invalid_fixed");
	assert_eq!(rejected.revision(), opened.revision());

	let replace = session_request(&opened, json!({"kind": "replace_raw"}));
	execute(&mut engine, replace.clone(), Some(b"<Bank>")).assert_error("invalid_document");
	assert_eq!(
		session_command(&mut engine, &opened, "raw")
			.bytes
			.as_deref(),
		Some(BANK)
	);

	let replacement = b"<Bank><Section name=\"Other\"/></Bank>";
	let changed = execute(&mut engine, replace, Some(replacement));
	assert_eq!(changed.revision(), "1");
	assert_eq!(changed.payload("snapshot")["sections"][0]["name"], "Other");
	let raw = session_command(&mut engine, &changed, "raw");
	assert_eq!(raw.bytes.as_deref(), Some(replacement.as_slice()));
}

#[test]
fn dirty_document_requires_explicit_discard_and_failed_open_preserves_identity() {
	let mut engine = Engine::default();
	let first = open(&mut engine, BANK);
	open(&mut engine, b"<Bank>").assert_error("invalid_document");
	assert_eq!(
		session_command(&mut engine, &first, "snapshot").revision(),
		"0"
	);

	let second = open(&mut engine, BANK);
	assert_ne!(first.document(), second.document());
	let expired = session_command(&mut engine, &first, "snapshot");
	expired.assert_error("unknown_document");
	assert_eq!(expired.control["document_id"], Value::Null);
	assert_eq!(expired.control["revision"], Value::Null);

	let changed = execute(
		&mut engine,
		session_request(&second, json!({"kind": "replace_raw"})),
		Some(b"<Bank/>"),
	);
	open(&mut engine, BANK).assert_error("dirty_document");
	session_command(&mut engine, &changed, "close").assert_error("dirty_document");
	let closed = execute(
		&mut engine,
		session_request(
			&changed,
			json!({"kind": "close", "payload": {"discard": true}}),
		),
		None,
	);
	assert_eq!(closed.payload("closed"), &json!({}));
	assert_eq!(closed.document(), changed.document());
	assert_eq!(closed.revision(), changed.revision());
	session_command(&mut engine, &changed, "snapshot").assert_error("unknown_document");
}

#[test]
fn explicit_discard_switches_only_after_the_new_bank_opens() {
	let mut engine = Engine::default();
	let opened = open(&mut engine, BANK);
	let edited_bytes = b"<Bank><Section name=\"In progress\"/></Bank>";
	let dirty = execute(
		&mut engine,
		session_request(&opened, json!({"kind": "replace_raw"})),
		Some(edited_bytes),
	);
	assert_eq!(dirty.payload("snapshot")["dirty"], true);
	let open_next = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "next",
		"command": {"kind": "open", "payload": {"discard_current": true}}
	});
	execute(&mut engine, open_next.clone(), Some(b"<Bank>")).assert_error("invalid_document");
	let restored = session_command(&mut engine, &dirty, "snapshot");
	assert_eq!(restored.payload("snapshot"), dirty.payload("snapshot"));
	assert_eq!(
		session_command(&mut engine, &dirty, "raw").bytes.as_deref(),
		Some(edited_bytes.as_slice())
	);

	let mut malformed_option = open_next.clone();
	malformed_option["command"]["payload"]["discard_current"] = Value::Null;
	execute(&mut engine, malformed_option, Some(BANK)).assert_error("invalid_request");
	assert_eq!(
		session_command(&mut engine, &dirty, "snapshot").revision(),
		dirty.revision()
	);

	let replacement = execute(&mut engine, open_next, Some(BANK));
	assert_eq!(replacement.revision(), "0");
	assert_ne!(replacement.document(), dirty.document());
	assert_eq!(replacement.payload("snapshot")["dirty"], false);
	session_command(&mut engine, &dirty, "raw").assert_error("unknown_document");
	assert_eq!(
		session_command(&mut engine, &replacement, "raw")
			.bytes
			.as_deref(),
		Some(BANK)
	);
}

#[test]
fn prepared_bytes_are_separate_and_do_not_clear_dirty_state() {
	let mut engine = Engine::default();
	let opened = open(&mut engine, b"<Bank/>");
	let bank = br#"<Bank><Section name="A"><Key name="B"><Value int="01"/></Key></Section></Bank>"#;
	let changed = execute(
		&mut engine,
		session_request(&opened, json!({"kind": "replace_raw"})),
		Some(bank),
	);
	let preserved = session_command(&mut engine, &changed, "prepare");
	assert_eq!(
		preserved.payload("prepared")["signature_status"],
		"unsigned"
	);
	assert_eq!(preserved.bytes.as_deref(), Some(bank.as_slice()));

	let signed = execute(
		&mut engine,
		session_request(
			&changed,
			json!({"kind": "prepare", "payload": {"context": serde_json::from_str::<Value>(CONTEXT).unwrap()}}),
		),
		None,
	);
	assert_eq!(signed.payload("prepared")["signature_status"], "valid");
	assert_eq!(signed.revision(), changed.revision());
	let signed_bytes = signed.bytes.expect("prepared signature bytes");
	let signature = sc2bank::parse(&signed_bytes, sc2bank::Limits::default()).expect("signed bank");
	assert!(signature.signature().is_some());
	assert_eq!(
		session_command(&mut engine, &changed, "snapshot").payload("snapshot")["dirty"],
		true
	);
	assert_eq!(
		session_command(&mut engine, &changed, "raw")
			.bytes
			.as_deref(),
		Some(bank.as_slice())
	);
}

#[test]
fn malformed_controls_cannot_mutate_and_do_not_echo_untrusted_content() {
	let mut engine = Engine::default();
	let opened = open(&mut engine, BANK);
	let valid_id = first_value(opened.payload("snapshot"))["id"]
		.as_str()
		.expect("value id");
	let malformed = [
		r#"{"kind":"not_a_command"}"#.to_owned(),
		r#"{"kind":"raw","payload":{}}"#.to_owned(),
		r#"{"kind":"prepare","payload":null}"#.to_owned(),
		r#"{"kind":"prepare","payload":{"context":null}}"#.to_owned(),
		r#"{"kind":"prepare","payload":{"context":["1-S2-1-111","1-S2-1-222","Stats"]}}"#
			.to_owned(),
		r#"{"kind":"prepare","payload":[null]}"#.to_owned(),
		r#"["prepare",{}]"#.to_owned(),
		format!(r#"{{"kind":"set_values","payload":{{"edits":[["{valid_id}","3"]]}}}}"#),
		format!(
			r#"{{"kind":"set_values","payload":{{"edits":[{{"id":"{valid_id}","value":9007199254740993}}]}}}}"#
		),
		format!(
			r#"{{"kind":"set_values","payload":{{"edits":[{{"id":"{valid_id}","value":"\uD800"}}]}}}}"#
		),
		format!(
			r#"{{"kind":"set_values","payload":{{"edits":[{{"id":"{valid_id}","value":"PRIVATE","value":"02"}}]}}}}"#
		),
		r#"{"kind":"close","payload":{"discard":false,"extra":true}}"#.to_owned(),
	];
	for command in malformed {
		let control = format!(
			r#"{{"schema_version":1,"request_id":"checked","document_id":"{}","expected_revision":"0","command":{command}}}"#,
			opened.document()
		);
		let error = Reply::decode(engine.dispatch(&control, None));
		error.assert_error("invalid_request");
		assert_eq!(error.control["request_id"], "checked");
		assert!(!error.control.to_string().contains("PRIVATE"));
	}

	let malformed_heads = [
		r#"[1,"id",null,null,{"kind":"open"}]"#,
		r#"{"schema_version":1,"request_id":"a","request_id":"b","command":{"kind":"open"}}"#,
		r#"{"schema_version":1,"request_id":"","command":{"kind":"open"}}"#,
	];
	for request in malformed_heads {
		let error = Reply::decode(engine.dispatch(request, None));
		error.assert_error("invalid_request");
		assert_eq!(error.control["request_id"], Value::Null);
	}
	let version = Reply::decode(engine.dispatch(
		r#"{"schema_version":2,"request_id":"version","command":{"kind":"open"}}"#,
		None,
	));
	version.assert_error("unsupported_version");
	assert_eq!(version.control["request_id"], "version");
	let mut revision = session_request(&opened, json!({"kind": "snapshot"}));
	revision["expected_revision"] = json!("18446744073709551616");
	execute(&mut engine, revision, None).assert_error("invalid_revision");
	assert_eq!(
		session_command(&mut engine, &opened, "raw")
			.bytes
			.as_deref(),
		Some(BANK)
	);
}

#[test]
fn control_bytes_attachments_and_operation_counts_are_bounded() {
	let mut engine = Engine::default();
	let huge_control = " ".repeat(MAX_CONTROL_BYTES + 1);
	Reply::decode(engine.dispatch(&huge_control, None)).assert_error("request_too_large");
	let large_attachment = vec![b' '; max_attachment_bytes() + 1];
	open(&mut engine, &large_attachment).assert_error("attachment_too_large");
	let opened = open(&mut engine, BANK);
	let edits = vec![json!({"id": "1", "value": "02"}); MAX_OPERATIONS + 1];
	execute(
		&mut engine,
		session_request(
			&opened,
			json!({"kind": "set_values", "payload": { "edits": edits }}),
		),
		None,
	)
	.assert_error("too_many_edits");
	execute(
		&mut engine,
		session_request(&opened, json!({"kind": "raw"})),
		Some(&[]),
	)
	.assert_error("unexpected_attachment");
	session_command(&mut engine, &opened, "replace_raw").assert_error("missing_attachment");
	assert_eq!(
		session_command(&mut engine, &opened, "snapshot").revision(),
		"0"
	);
}
