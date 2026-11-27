use serde_json::{Value, json};
use starbank_editor::{Engine, WireReply};

const DEFINITION: &[u8] = include_bytes!("../../starbank-profiles/tests/fixtures/training.json");
const BANK: &[u8] = include_bytes!("../../starbank-profiles/tests/fixtures/training.sc2bank");
const MAX_OVERSIZE_PROFILE: usize = 64 * 1024 + 1;

fn exchange(
	engine: &mut Engine,
	target: Option<(&str, &str)>,
	command: Value,
	attachment: Option<&[u8]>,
) -> WireReply {
	let mut message = json!({ "schema_version": 1, "request_id": "req-test", "command": command });
	if let Some((document_id, revision)) = target {
		message["document_id"] = json!(document_id);
		message["expected_revision"] = json!(revision);
	}
	engine.dispatch(&message.to_string(), attachment)
}

fn dispatch(
	engine: &mut Engine,
	target: Option<(&str, &str)>,
	command: Value,
	attachment: Option<&[u8]>,
) -> Value {
	let reply = exchange(engine, target, command, attachment);
	assert!(reply.bytes.is_none());
	serde_json::from_str(&reply.json).unwrap()
}

fn open(engine: &mut Engine, context: bool) -> String {
	let mut command = json!({ "kind": "open" });
	if context {
		command["payload"] = json!({ "context": {
			"author": "9-S2-1-123", "player": "9-S2-1-456", "bank_name": "TrainingBank"
		}});
	}
	dispatch(engine, None, command, Some(BANK))["document_id"]
		.as_str()
		.unwrap()
		.to_owned()
}

fn profile_file() -> Value {
	json!({ "kind": "profile_load", "payload": { "format": "json" } })
}

fn manual_profile_file() -> Value {
	json!({ "kind": "profile_load", "payload": { "format": "json", "identity": {
		"author": "9-S2-1-123",
		"bank_name": "TrainingBank"
	} } })
}

fn profile_preview(value: &str) -> Value {
	json!({ "kind": "profile_preview", "payload": {
		"profile_id": "synthetic-training",
		"field_id": "score",
		"input": { "kind": "text", "value": value }
	} })
}

fn payload(reply: &Value) -> &Value {
	&reply["result"]["payload"]
}

#[test]
fn profile_preview_applies_all_changes_exactly_and_undoes_once() {
	let mut engine = Engine::default();
	let id = open(&mut engine, true);
	let target = Some((id.as_str(), "0"));
	let initial = dispatch(
		&mut engine,
		target,
		json!({"kind":"profile_snapshot"}),
		None,
	);
	assert_eq!(payload(&initial)["status"], "unloaded");
	let loaded = dispatch(&mut engine, target, profile_file(), Some(DEFINITION));
	assert_eq!(loaded["result"]["kind"], "profile");
	assert_eq!(loaded["revision"], "0");
	let profile = payload(&loaded);
	assert_eq!(profile["status"], "matched");
	assert_eq!(profile["profile_id"], "synthetic-training");
	assert_eq!(profile["groups"][0]["fields"][0]["read_only"], true);
	assert_eq!(profile["groups"][1]["fields"][0]["kind"], "checkbox");
	assert_eq!(profile["groups"][1]["fields"][0]["checked"], true);

	let preview = dispatch(
		&mut engine,
		target,
		profile_preview("9,007,199,254,740,993.50"),
		None,
	);
	assert_eq!(preview["revision"], "0");
	assert_eq!(preview["result"]["kind"], "profile_preview");
	let changes = payload(&preview)["changes"].as_array().unwrap();
	assert_eq!(changes.len(), 3);
	assert_eq!(changes[0]["value"], "54043195528445961.25");
	assert_eq!(changes[1]["previous"], "01.00");
	assert_eq!(changes[1]["value"], "9007199254740993.50");
	assert_eq!(changes[2]["value"], "18014398509481987.25");
	let apply =
		json!({"kind":"profile_apply","payload":{"preview_id":payload(&preview)["preview_id"]}});
	let applied = dispatch(&mut engine, target, apply.clone(), None);
	assert_eq!(applied["result"]["kind"], "snapshot");
	assert_eq!(applied["revision"], "1");
	assert_eq!(payload(&applied)["dirty"], true);
	assert_eq!(
		payload(&applied)["sections"][1]["keys"][0]["values"][0]["value"],
		"9007199254740993.50"
	);
	let replay = dispatch(&mut engine, Some((&id, "1")), apply, None);
	assert_eq!(payload(&replay)["code"], "stale_profile_preview");
	let undone = dispatch(&mut engine, Some((&id, "1")), json!({"kind":"undo"}), None);
	assert_eq!(undone["revision"], "2");
	assert_eq!(payload(&undone)["dirty"], false);
	assert_eq!(payload(&undone)["can_undo"], false);
}

#[test]
fn loading_rejects_bad_input_and_attestation_mismatch_without_granting_signing() {
	let mut engine = Engine::default();
	let id = open(&mut engine, false);
	let target = Some((id.as_str(), "0"));
	let outside = dispatch(&mut engine, target, profile_file(), Some(DEFINITION));
	assert_eq!(payload(&outside)["status"], "missing_context");
	let verified = dispatch(&mut engine, target, manual_profile_file(), Some(DEFINITION));
	assert_eq!(payload(&verified)["status"], "matched");
	let snapshot = dispatch(&mut engine, target, json!({"kind":"snapshot"}), None);
	assert_eq!(payload(&snapshot)["signature_status"], "unsigned");

	let preview = dispatch(&mut engine, target, profile_preview("-01.00"), None);
	let apply =
		json!({"kind":"profile_apply","payload":{"preview_id":payload(&preview)["preview_id"]}});
	let applied = dispatch(&mut engine, target, apply, None);
	assert_eq!(applied["revision"], "1");
	assert_eq!(payload(&applied)["signature_status"], "unsigned");

	let id = open_replacing(&mut engine);
	let target = Some((id.as_str(), "0"));
	let mut conflict = manual_profile_file();
	conflict["payload"]["identity"]["author"] = "9-S2-1-789".into();
	assert_eq!(
		payload(&dispatch(&mut engine, target, conflict, Some(DEFINITION)))["code"],
		"invalid_profile_identity"
	);
	let too_large = vec![0; MAX_OVERSIZE_PROFILE];
	for (source, code) in [
		(Some(too_large.as_slice()), "attachment_too_large"),
		(Some(b"{}" as &[u8]), "invalid_profile"),
		(None, "missing_attachment"),
	] {
		assert_eq!(
			payload(&dispatch(&mut engine, target, profile_file(), source))["code"],
			code
		);
	}
	let state = dispatch(
		&mut engine,
		target,
		json!({"kind":"profile_snapshot"}),
		None,
	);
	assert_eq!(payload(&state)["status"], "unloaded");
}

fn open_replacing(engine: &mut Engine) -> String {
	dispatch(
		engine,
		None,
		json!({"kind":"open","payload":{
			"discard_current": true,
			"context":{"author":"9-S2-1-123","player":"9-S2-1-456","bank_name":"TrainingBank"}
		}}),
		Some(BANK),
	)["document_id"]
		.as_str()
		.unwrap()
		.to_owned()
}

#[test]
fn clearing_replacing_or_invalidating_the_profile_rejects_old_previews() {
	let mut engine = Engine::default();
	let id = open(&mut engine, true);
	let target = Some((id.as_str(), "0"));
	dispatch(&mut engine, target, profile_file(), Some(DEFINITION));
	let preview = dispatch(&mut engine, target, profile_preview("5.00"), None);
	let apply =
		json!({"kind":"profile_apply","payload":{"preview_id":payload(&preview)["preview_id"]}});
	let unknown = dispatch(&mut engine, target, json!({"kind":"profile_clear"}), None);
	assert_eq!(payload(&unknown)["status"], "unloaded");
	let denied = dispatch(&mut engine, target, apply.clone(), None);
	assert_eq!(payload(&denied)["code"], "stale_profile_preview");

	dispatch(&mut engine, target, profile_file(), Some(DEFINITION));
	let advanced = dispatch(&mut engine, target, profile_preview("20.00"), None);
	let advanced_apply =
		json!({"kind":"profile_apply","payload":{"preview_id":payload(&advanced)["preview_id"]}});
	let replacement = core::str::from_utf8(DEFINITION)
		.unwrap()
		.replace("fixture-v1", "fixture-v2");
	let incompatible = dispatch(
		&mut engine,
		target,
		profile_file(),
		Some(replacement.as_bytes()),
	);
	assert_eq!(payload(&incompatible)["status"], "incompatible");
	let denied = dispatch(&mut engine, target, advanced_apply, None);
	assert_eq!(payload(&denied)["code"], "stale_profile_preview");
	let malformed = dispatch(&mut engine, target, profile_preview("1,,00"), None);
	assert_eq!(payload(&malformed)["code"], "profile_unavailable");
}

#[test]
fn profile_identity_does_not_adopt_signing_context() {
	use sc2bank::{Limits, parse};
	use starbank_signature::{SigningContext, resign};

	let mut engine = Engine::default();
	let raw = std::str::from_utf8(BANK)
		.unwrap()
		.replace("<Extension><Unprojected /></Extension>", "");
	let bank = parse(raw.as_bytes(), Limits::default()).unwrap();
	let context = SigningContext::new("9-S2-1-123", "9-S2-1-456", "TrainingBank").unwrap();
	let signed = resign(&bank, &context, Limits::default()).unwrap();
	let opened = dispatch(
		&mut engine,
		None,
		json!({"kind":"open"}),
		Some(signed.source()),
	);
	let target = Some((opened["document_id"].as_str().unwrap(), "0"));
	assert_eq!(payload(&opened)["signature_status"], "missing_context");
	let loaded = dispatch(&mut engine, target, manual_profile_file(), Some(DEFINITION));
	assert_eq!(payload(&loaded)["status"], "matched");
	let after = dispatch(&mut engine, target, json!({"kind":"snapshot"}), None);
	assert_eq!(payload(&after)["signature_status"], "missing_context");
}

fn checkbox_bank(kind: &str, value: &str) -> String {
	format!(
		r#"<Bank><Section name="Options"><Key name="Enabled"><Value {kind}="{value}"/></Key></Section></Bank>"#
	)
}

fn checkbox_profile(checked: &str, unchecked: &str) -> Vec<u8> {
	serde_json::to_vec(&json!({
		"version": 1,
		"id": "synthetic-checkbox",
		"identity": {"author":"9-S2-1-123", "bank":"TrainingBank"},
		"groups": [{
			"label": "Options",
			"fields": [{
				"id": "enabled",
				"label": "Enabled",
				"path": {"section":{"name":"Options"}, "key":{"name":"Enabled"}},
				"control": {
					"kind":"checkbox", "storage":"bank_value",
					"checked":checked, "unchecked":unchecked
				}
			}]
		}]
	}))
	.unwrap()
}

fn checkbox_preview(checked: bool) -> Value {
	json!({"kind":"profile_preview","payload":{
		"profile_id":"synthetic-checkbox","field_id":"enabled",
		"input":{"kind":"checked","value":checked}
	}})
}

#[test]
fn invalid_integer_checkbox_endpoint_cannot_match_or_change_the_bank() {
	let bank = checkbox_bank("int", "0");
	let mut engine = Engine::default();
	let opened = dispatch(
		&mut engine,
		None,
		json!({"kind":"open"}),
		Some(bank.as_bytes()),
	);
	let target = Some((opened["document_id"].as_str().unwrap(), "0"));

	for invalid in ["NaN", "1,000", "170141183460469231731687303715884105728"] {
		for (checked, unchecked, requested) in [(invalid, "0", true), ("0", invalid, false)] {
			let definition = checkbox_profile(checked, unchecked);
			let loaded = dispatch(
				&mut engine,
				target,
				manual_profile_file(),
				Some(&definition),
			);
			assert_eq!(loaded["revision"], "0");
			assert_eq!(payload(&loaded)["status"], "incompatible");
			let preview = dispatch(&mut engine, target, checkbox_preview(requested), None);
			assert_eq!(payload(&preview)["code"], "profile_unavailable");
			assert_eq!(preview["revision"], "0");
		}
	}

	let snapshot = dispatch(&mut engine, target, json!({"kind":"snapshot"}), None);
	assert_eq!(snapshot["revision"], "0");
	assert_eq!(payload(&snapshot)["dirty"], false);
	assert_eq!(payload(&snapshot)["can_undo"], false);
	let raw = exchange(&mut engine, target, json!({"kind":"raw"}), None);
	assert_eq!(raw.bytes.as_deref(), Some(bank.as_bytes()));
}

#[test]
fn valid_bank_value_checkbox_toggles_keep_exact_storage_lexemes() {
	for (kind, checked, unchecked) in [
		("int", "0001", "0"),
		("flag", "yes", "no"),
		("string", "1,000", "literal"),
	] {
		let bank = checkbox_bank(kind, unchecked);
		let mut engine = Engine::default();
		let opened = dispatch(
			&mut engine,
			None,
			json!({"kind":"open"}),
			Some(bank.as_bytes()),
		);
		let id = opened["document_id"].as_str().unwrap();
		let definition = checkbox_profile(checked, unchecked);
		let loaded = dispatch(
			&mut engine,
			Some((id, "0")),
			manual_profile_file(),
			Some(&definition),
		);
		assert_eq!(payload(&loaded)["status"], "matched");

		for (revision, next, checked_now, expected) in
			[("0", "1", true, checked), ("1", "2", false, unchecked)]
		{
			let target = Some((id, revision));
			let preview = dispatch(&mut engine, target, checkbox_preview(checked_now), None);
			assert_eq!(payload(&preview)["changes"][0]["value"], expected);
			let applied = dispatch(
				&mut engine,
				target,
				json!({
					"kind":"profile_apply","payload":{"preview_id":payload(&preview)["preview_id"]}
				}),
				None,
			);
			assert_eq!(applied["result"]["kind"], "snapshot");
			assert_eq!(applied["revision"], next);

			let raw = exchange(&mut engine, Some((id, next)), json!({"kind":"raw"}), None);
			assert_eq!(
				raw.bytes.as_deref(),
				Some(checkbox_bank(kind, expected).as_bytes())
			);
		}
	}
}
