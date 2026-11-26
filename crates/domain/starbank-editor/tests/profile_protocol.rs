use serde_json::{Value, json};
use starbank_editor::Engine;

const DEFINITION: &[u8] = include_bytes!("../../starbank-profiles/tests/fixtures/training.json");
const BANK: &[u8] = include_bytes!("../../starbank-profiles/tests/fixtures/training.sc2bank");
const MAX_OVERSIZE_PROFILE: usize = 64 * 1024 + 1;

fn dispatch(
	engine: &mut Engine,
	target: Option<(&str, &str)>,
	command: Value,
	attachment: Option<&[u8]>,
) -> Value {
	let mut message = json!({ "schema_version": 1, "request_id": "req-test", "command": command });
	if let Some((document_id, revision)) = target {
		message["document_id"] = json!(document_id);
		message["expected_revision"] = json!(revision);
	}
	let reply = engine.dispatch(&message.to_string(), attachment);
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
