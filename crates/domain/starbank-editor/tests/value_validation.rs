use sc2bank::Limits;
use serde_json::{Value, json};
use starbank_core::Revision;
use starbank_editor::{
	EditorError, Engine, MAX_CONTROL_BYTES, MAX_OPERATIONS, OccurrenceId, PROTOCOL_VERSION,
	Session, ValueChange, ValueIssue, ValueIssueCode,
};

const BANK: &[u8] = b"<Bank><Section name='Stats'><Key name='Test'><A int='7'/><B fixed='2.00'/><C string='A, B'/><D future='unchanged'/><E/><F int='legacy'/></Key></Section></Bank>";

fn dispatch(engine: &mut Engine, request: &Value, attachment: Option<&[u8]>) -> Value {
	let reply = engine.dispatch(&request.to_string(), attachment);
	assert!(reply.bytes.is_none());
	serde_json::from_str(&reply.json).unwrap()
}

fn open(engine: &mut Engine) -> Value {
	dispatch(
		engine,
		&json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "open",
			"command": { "kind": "open" },
		}),
		Some(BANK),
	)
}

fn request(opened: &Value, command: Value) -> Value {
	json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "draft-check",
		"document_id": opened["document_id"],
		"expected_revision": opened["revision"],
		"command": command,
	})
}

fn validate(opened: &Value, edits: Value) -> Value {
	request(
		opened,
		json!({ "kind": "validate_values", "payload": { "edits": edits } }),
	)
}

#[test]
fn reports_draft_issues_in_input_order_without_replacing_the_revision_or_snapshot() {
	let mut engine = Engine::default();
	let opened = open(&mut engine);
	let snapshot = &opened["result"]["payload"];
	let section = &snapshot["sections"][0];
	let values = &section["keys"][0]["values"];

	let invalid = validate(
		&opened,
		json!([
			{ "id": values[4]["id"], "value": "opaque" },
			{ "id": values[1]["id"], "value": "12,34.0" },
			{ "id": section["id"], "value": "valid value, wrong occurrence kind" },
			{ "id": values[2]["id"], "value": "private string, still valid" },
			{ "id": values[3]["id"], "value": "unsupported" },
			{ "id": values[4]["id"], "value": "duplicate" },
			{ "id": values[0]["id"], "value": "1,,2" },
		]),
	);
	let reply = dispatch(&mut engine, &invalid, None);
	assert_eq!(reply["request_id"], invalid["request_id"]);
	assert_eq!(reply["document_id"], opened["document_id"]);
	assert_eq!(reply["revision"], opened["revision"]);
	assert_eq!(reply["result"]["kind"], "value_validation");
	let issues = reply["result"]["payload"]["issues"].as_array().unwrap();
	let expected = [
		(&values[4]["id"], "opaque_value"),
		(&values[1]["id"], "invalid_fixed"),
		(&section["id"], "unknown_value"),
		(&values[3]["id"], "unsupported_kind"),
		(&values[4]["id"], "duplicate_value"),
		(&values[0]["id"], "invalid_integer"),
	];
	assert_eq!(issues.len(), expected.len());
	for (issue, (id, code)) in issues.iter().zip(expected) {
		assert_eq!(&issue["id"], id);
		assert_eq!(issue["code"], code);
		assert!(
			issue["message"]
				.as_str()
				.is_some_and(|text| !text.is_empty())
		);
	}
	assert!(!reply.to_string().contains("private string"));

	let valid_edits = json!([
		{ "id": values[0]["id"], "value": "9,007,199,254,740,993" },
		{ "id": values[1]["id"], "value": "-1,234.500" },
		{ "id": values[2]["id"], "value": "A, B, C" },
		{ "id": values[3]["id"], "value": "unchanged" },
		{ "id": values[5]["id"], "value": "legacy" },
	]);
	for edits in [valid_edits.clone(), json!([])] {
		let reply = dispatch(&mut engine, &validate(&opened, edits), None);
		assert_eq!(reply["result"]["kind"], "value_validation");
		assert_eq!(reply["result"]["payload"]["issues"], json!([]));
		assert_eq!(reply["revision"], opened["revision"]);
	}
	let before = dispatch(
		&mut engine,
		&request(&opened, json!({ "kind": "snapshot" })),
		None,
	);
	assert_eq!(&before["result"]["payload"], snapshot);
	let raw_request = request(&opened, json!({ "kind": "raw" }));
	let raw = engine.dispatch(&raw_request.to_string(), None);
	assert_eq!(raw.bytes.as_deref(), Some(BANK));

	let changed = dispatch(
		&mut engine,
		&request(
			&opened,
			json!({ "kind": "set_values", "payload": { "edits": valid_edits } }),
		),
		None,
	);
	let committed = &changed["result"]["payload"]["sections"][0]["keys"][0]["values"];
	assert_eq!(committed[0]["value"], "9007199254740993");
	assert_eq!(committed[1]["value"], "-1234.500");
	assert_eq!(committed[2]["value"], "A, B, C");
	assert_eq!(changed["result"]["payload"]["can_undo"], true);
	assert_ne!(changed["revision"], opened["revision"]);
	assert_eq!(
		dispatch(&mut engine, &invalid, None)["result"]["payload"]["code"],
		"stale_revision",
	);
}

#[test]
fn validation_keeps_the_same_envelope_and_resource_limits_as_edits() {
	let mut engine = Engine::default();
	let opened = open(&mut engine);
	let invalid = validate(&opened, json!([{ "id": "01", "value": "2" }]));
	assert_eq!(
		dispatch(&mut engine, &invalid, None)["result"]["payload"]["code"],
		"invalid_occurrence",
	);
	let empty = validate(&opened, json!([]));
	assert_eq!(
		dispatch(&mut engine, &empty, Some(&[]))["result"]["payload"]["code"],
		"unexpected_attachment",
	);
	let invalid = validate(
		&opened,
		json!([{ "id": "3", "value": 9007199254740993u64 }]),
	);
	assert_eq!(
		dispatch(&mut engine, &invalid, None)["result"]["payload"]["code"],
		"invalid_request",
	);
	let id = &opened["result"]["payload"]["sections"][0]["keys"][0]["values"][0]["id"];
	let edit = json!({ "id": id, "value": "1" });
	let boundary = validate(&opened, json!(vec![&edit; MAX_OPERATIONS]));
	let reply = dispatch(&mut engine, &boundary, None);
	assert_eq!(reply["result"]["kind"], "value_validation");
	assert_eq!(
		reply["result"]["payload"]["issues"]
			.as_array()
			.unwrap()
			.len(),
		MAX_OPERATIONS - 1,
	);
	let overflow = validate(&opened, json!(vec![&edit; MAX_OPERATIONS + 1]));
	assert_eq!(
		dispatch(&mut engine, &overflow, None)["result"]["payload"]["code"],
		"too_many_edits",
	);

	let mut control = empty.to_string();
	control.extend(std::iter::repeat_n(' ', MAX_CONTROL_BYTES - control.len()));
	let reply = engine.dispatch(&control, None);
	assert!(reply.bytes.is_none());
	assert_eq!(
		serde_json::from_str::<Value>(&reply.json).unwrap()["result"]["kind"],
		"value_validation",
	);
	control.push(' ');
	let reply = engine.dispatch(&control, None);
	assert!(reply.bytes.is_none());
	assert_eq!(
		serde_json::from_str::<Value>(&reply.json).unwrap()["result"]["payload"]["code"],
		"request_too_large",
	);
}

#[test]
fn scalar_limit_is_an_issue_for_one_draft_and_a_commit_error_for_the_batch() {
	let limits = Limits {
		scalar_bytes: 12,
		..Limits::default()
	};
	let mut session = Session::open(BANK, limits, None).unwrap();
	let ids: Vec<_> = session.document().sections()[0].keys()[0]
		.values()
		.iter()
		.map(|value| session.id_of_value(value.id()).unwrap())
		.collect();
	let unknown = "99999".parse::<OccurrenceId>().unwrap();
	let edits = [
		ValueChange {
			id: ids[2],
			value: "ééééééé",
		},
		ValueChange {
			id: unknown,
			value: "0",
		},
		ValueChange {
			id: ids[0],
			value: "123456789012",
		},
	];
	assert_eq!(
		session.validate_values(Revision::INITIAL, &edits).unwrap(),
		[
			ValueIssue {
				id: ids[2],
				code: ValueIssueCode::ValueTooLarge
			},
			ValueIssue {
				id: unknown,
				code: ValueIssueCode::UnknownValue
			},
		],
	);
	assert!(matches!(
		session.set_values(Revision::INITIAL, &edits),
		Err(EditorError::EditLimit)
	));
	assert_eq!(session.revision(), Revision::INITIAL);
	assert_eq!(session.document().source(), BANK);
	assert!(!session.can_undo());
	assert!(!session.can_redo());
	assert!(!session.is_dirty());
}
