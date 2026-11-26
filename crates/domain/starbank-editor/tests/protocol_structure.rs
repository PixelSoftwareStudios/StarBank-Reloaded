use serde_json::{Value, json};
use starbank_editor::{Engine, PROTOCOL_VERSION};

const BANK: &[u8] = br#"<Bank>
	<Section name="Original">
		<Key name="Value group"><Value fixed="2.50"/></Key>
	</Section>
</Bank>"#;

struct SessionWire {
	engine: Engine,
	document_id: String,
	revision: String,
	snapshot: Value,
}

impl SessionWire {
	fn open() -> Self {
		let mut engine = Engine::default();
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "open",
			"command": {"kind": "open"},
		});
		let result = engine.dispatch(&control.to_string(), Some(BANK));
		let result: Value = serde_json::from_str(&result.json).unwrap();
		assert_eq!(result["result"]["kind"], "snapshot", "{result}");
		Self {
			engine,
			document_id: string(&result["document_id"]),
			revision: string(&result["revision"]),
			snapshot: result["result"]["payload"].clone(),
		}
	}

	fn request(&mut self, command: Value) -> Value {
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "session",
			"document_id": self.document_id,
			"expected_revision": self.revision,
			"command": command,
		});
		let result = self.engine.dispatch(&control.to_string(), None);
		assert!(result.bytes.is_none());
		let result: Value = serde_json::from_str(&result.json).unwrap();
		if result["result"]["kind"] == "snapshot" {
			self.revision = string(&result["revision"]);
			self.snapshot = result["result"]["payload"].clone();
		}
		result
	}

	fn change(&mut self, payload: Value) {
		let result = self.request(json!({"kind": "structure", "payload": payload}));
		assert_eq!(result["result"]["kind"], "snapshot", "{result}");
	}

	fn diff(&mut self) -> Value {
		let result = self.request(json!({"kind": "diff"}));
		assert_eq!(result["result"]["kind"], "diff", "{result}");
		assert_eq!(result["revision"], self.revision);
		result["result"]["payload"].clone()
	}
}

fn string(value: &Value) -> String {
	value.as_str().expect("expected opaque string").to_owned()
}

#[test]
fn structural_commands_use_stable_occurrences_and_exact_values() {
	let mut wire = SessionWire::open();
	let original_id = string(&wire.snapshot["sections"][0]["id"]);

	wire.change(json!({"kind": "add_section", "name": "Temporary"}));
	let section_id = string(&wire.snapshot["sections"][1]["id"]);
	wire.change(json!({"kind": "rename_section", "id": section_id, "name": "Draft"}));
	assert_eq!(wire.snapshot["sections"][1]["id"], section_id);
	assert_eq!(wire.snapshot["sections"][1]["name"], "Draft");

	wire.change(json!({"kind": "add_key", "section_id": section_id, "name": "Totals"}));
	let key_id = string(&wire.snapshot["sections"][1]["keys"][0]["id"]);
	wire.change(json!({"kind": "rename_key", "id": key_id, "name": "Scores"}));
	assert_eq!(wire.snapshot["sections"][1]["keys"][0]["id"], key_id);
	assert_eq!(wire.snapshot["sections"][1]["keys"][0]["name"], "Scores");

	wire.change(json!({
		"kind": "add_value",
		"key_id": key_id,
		"name": "Value",
		"value_kind": "fixed",
		"value": "9,007,199,254,740,993.50",
	}));
	let value_id = string(&wire.snapshot["sections"][1]["keys"][0]["values"][0]["id"]);
	assert_eq!(
		wire.snapshot["sections"][1]["keys"][0]["values"][0]["value"],
		"9007199254740993.50"
	);
	wire.change(json!({"kind": "rename_value", "id": value_id, "name": "Detail"}));
	wire.change(json!({
		"kind": "set_value_type",
		"id": value_id,
		"value_kind": "int",
		"value": "1,002",
	}));
	let value = &wire.snapshot["sections"][1]["keys"][0]["values"][0];
	assert_eq!(
		value,
		&json!({"id": value_id, "name": "Detail", "kind": "int", "value": "1002", "editable": true})
	);

	wire.change(json!({"kind": "remove_value", "id": value_id}));
	assert_eq!(wire.snapshot["sections"][1]["keys"][0]["values"], json!([]));
	wire.change(json!({"kind": "remove_key", "id": key_id}));
	assert_eq!(wire.snapshot["sections"][1]["keys"], json!([]));
	wire.change(json!({"kind": "remove_section", "id": section_id}));
	assert_eq!(wire.snapshot["sections"].as_array().unwrap().len(), 1);
	assert_eq!(wire.snapshot["sections"][0]["id"], original_id);

	let undone = wire.request(json!({"kind": "undo"}));
	assert_eq!(undone["result"]["kind"], "snapshot", "{undone}");
	assert_eq!(wire.snapshot["sections"][1]["id"], section_id);
}

#[test]
fn diff_reports_exact_lexemes_and_revert_restores_opening_occurrences() {
	let mut wire = SessionWire::open();
	let section_id = string(&wire.snapshot["sections"][0]["id"]);
	let key_id = string(&wire.snapshot["sections"][0]["keys"][0]["id"]);
	let value_id = string(&wire.snapshot["sections"][0]["keys"][0]["values"][0]["id"]);
	assert_eq!(wire.diff(), json!({"source_changed": false, "changes": []}));

	let updated = wire.request(json!({
		"kind": "set_values",
		"payload": {"edits": [{"id": value_id, "value": "9007199254740993.50"}]},
	}));
	assert_eq!(updated["result"]["kind"], "snapshot", "{updated}");
	let diff = wire.diff();
	assert_eq!(diff["source_changed"], true);
	assert_eq!(diff["changes"].as_array().unwrap().len(), 3);
	assert_eq!(
		diff["changes"][2],
		json!({
			"id": value_id,
			"kind": "value",
			"change": {"kind": "changed", "payload": {
				"before":{"parent":key_id,"name":"Value","value":{"kind":"fixed","text":"2.50"}},
				"after":{"parent":key_id,"name":"Value","value":{"kind":"fixed","text":"9007199254740993.50"}},
			}},
		})
	);

	let wrong_kind =
		wire.request(json!({"kind": "revert", "payload": {"kind": "key", "id": value_id}}));
	assert_eq!(
		wrong_kind["result"]["payload"]["code"],
		"revert_unavailable"
	);
	let reverted =
		wire.request(json!({"kind": "revert", "payload": {"kind": "value", "id": value_id}}));
	assert_eq!(reverted["result"]["kind"], "snapshot", "{reverted}");
	assert_eq!(wire.diff(), json!({"source_changed": false, "changes": []}));

	wire.change(json!({"kind": "add_key", "section_id": section_id, "name": "Temporary"}));
	let added_id = string(&wire.snapshot["sections"][0]["keys"][1]["id"]);
	assert_eq!(
		wire.diff()["changes"][1],
		json!({"id": added_id, "kind": "key", "change": {"kind": "added", "payload": {
			"parent": section_id, "name": "Temporary", "value": null
		}}})
	);
	let key_reverted =
		wire.request(json!({"kind": "revert", "payload": {"kind": "key", "id": added_id}}));
	assert_eq!(
		key_reverted["result"]["payload"]["code"],
		"revert_unavailable"
	);
	let section_reverted =
		wire.request(json!({"kind": "revert", "payload": {"kind": "section", "id": section_id}}));
	assert_eq!(
		section_reverted["result"]["kind"], "snapshot",
		"{section_reverted}"
	);
	assert_eq!(wire.diff(), json!({"source_changed": false, "changes": []}));

	wire.change(json!({"kind": "remove_value", "id": value_id}));
	assert_eq!(
		wire.diff()["changes"][2],
		json!({"id": value_id, "kind": "value", "change": {"kind": "removed", "payload": {
			"parent": key_id, "name": "Value", "value": {"kind": "fixed", "text": "2.50"}
		}}})
	);
	let key_reverted =
		wire.request(json!({"kind": "revert", "payload": {"kind": "key", "id": key_id}}));
	assert_eq!(key_reverted["result"]["kind"], "snapshot", "{key_reverted}");
	assert_eq!(
		wire.snapshot["sections"][0]["keys"][0]["values"][0]["id"],
		value_id
	);
	assert_eq!(wire.diff(), json!({"source_changed": false, "changes": []}));
}

#[test]
fn new_control_shapes_fail_closed() {
	let mut wire = SessionWire::open();
	let value_id = string(&wire.snapshot["sections"][0]["keys"][0]["values"][0]["id"]);
	for command in [
		json!({"kind": "structure", "payload": ["remove_value", value_id]}),
		json!({"kind": "structure", "payload": {"kind": "rename_value", "id": value_id, "name": "Changed", "extra": 1}}),
		json!({"kind": "structure", "payload": {"kind": "set_value_type", "id": value_id, "value_kind": "future", "value": "9"}}),
		json!({"kind": "revert", "payload": {"kind": "invalid", "id": value_id}}),
		json!({"kind": "diff", "payload": {}}),
	] {
		let result = wire.request(command);
		assert_eq!(result["result"]["kind"], "error", "{result}");
		assert_eq!(
			result["result"]["payload"]["code"], "invalid_request",
			"{result}"
		);
	}
	assert_eq!(wire.revision, "0");
	assert_eq!(wire.diff(), json!({"source_changed": false, "changes": []}));
}
