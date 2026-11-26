use serde_json::{Value, json};
use starbank_editor::{Engine, PROTOCOL_VERSION};

const ORIGINAL: &str = "<?xml version='1.0' encoding='UTF-16'?>\r\n<Bank><Section name='星'><Key name='值😀'><Value int='007'/></Key></Section></Bank>\r\n";
const CHANGED: &str = "<?xml version='1.0' encoding='UTF-16'?>\r\n<Bank><Section name='星'><Key name='值😀'><Value int='008'/></Key></Section></Bank>\r\n";

fn utf16(text: &str, little: bool) -> Vec<u8> {
	let units = std::iter::once(0xfeff).chain(text.encode_utf16());
	if little {
		units.flat_map(u16::to_le_bytes).collect()
	} else {
		units.flat_map(u16::to_be_bytes).collect()
	}
}

struct Session {
	engine: Engine,
	document: Value,
}

impl Session {
	fn open(bytes: &[u8]) -> Self {
		let mut engine = Engine::default();
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "open",
			"command": {"kind": "open"},
		});
		let reply = engine.dispatch(&control.to_string(), Some(bytes));
		let document: Value = serde_json::from_str(&reply.json).expect("encoded reply");
		assert_eq!(document["result"]["kind"], "snapshot", "{document}");
		Self { engine, document }
	}

	fn command(&mut self, kind: &str, bytes: Option<&[u8]>) -> (Value, Option<Vec<u8>>) {
		self.request(json!({"kind": kind}), bytes)
	}

	fn request(&mut self, command: Value, bytes: Option<&[u8]>) -> (Value, Option<Vec<u8>>) {
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "session",
			"document_id": self.document["document_id"],
			"expected_revision": self.document["revision"],
			"command": command,
		});
		let reply = self.engine.dispatch(&control.to_string(), bytes);
		let response: Value = serde_json::from_str(&reply.json).expect("encoded reply");
		if response["result"]["kind"] == "snapshot" {
			self.document = response.clone();
		}
		(response, reply.bytes)
	}
}

#[test]
fn raw_text_is_utf8_and_prepare_and_opening_backup_keep_native_bytes() {
	for (little, label) in [(true, "utf-16le"), (false, "utf-16be")] {
		let opening = utf16(ORIGINAL, little);
		let changed = utf16(CHANGED, little);
		let mut session = Session::open(&opening);

		let (raw, bytes) = session.command("raw_text", None);
		assert_eq!(raw["schema_version"], PROTOCOL_VERSION);
		assert_eq!(raw["result"]["kind"], "raw_text");
		assert_eq!(raw["result"]["payload"]["source_encoding"], label);
		let bytes = bytes.expect("UTF-8 attachment");
		assert_eq!(
			std::str::from_utf8(&bytes).unwrap(),
			format!("\u{feff}{ORIGINAL}")
		);
		assert_eq!(session.command("raw", None).1.unwrap(), opening);
		assert_eq!(session.command("prepare", None).1.unwrap(), opening);

		// Text input can omit the mandatory native BOM. Encoding reinserts it.
		let (reply, _) = session.command("replace_raw_text", Some(CHANGED.as_bytes()));
		assert_eq!(reply["result"]["kind"], "snapshot", "{reply}");
		assert_eq!(reply["result"]["payload"]["dirty"], true);
		assert_eq!(session.command("raw", None).1.unwrap(), changed);
		assert_eq!(session.command("prepare", None).1.unwrap(), changed);
		let (backup, original_bytes) = session.command("opening_raw", None);
		assert_eq!(backup["result"]["kind"], "raw", "{backup}");
		assert_eq!(backup["revision"], reply["revision"]);
		assert_eq!(original_bytes.unwrap(), opening);
		assert_eq!(
			session.command("snapshot", None).0["result"]["payload"]["dirty"],
			true
		);
		assert_eq!(
			session.command("undo", None).0["result"]["kind"],
			"snapshot"
		);
		assert_eq!(session.command("raw", None).1.unwrap(), opening);
	}
}

#[test]
fn raw_edits_reject_invalid_wire_text_and_cross_encoding_without_replacing_bytes() {
	let opening = utf16(ORIGINAL, true);
	let mut session = Session::open(&opening);

	let (invalid_text, _) = session.command("replace_raw_text", Some(&[0xff]));
	assert_eq!(
		invalid_text["result"]["payload"]["code"],
		"invalid_document"
	);
	let wrong_declaration = CHANGED.replace("UTF-16", "UTF-8");
	let (declared, _) = session.command("replace_raw_text", Some(wrong_declaration.as_bytes()));
	assert_eq!(declared["result"]["kind"], "error", "{declared}");
	let (changed_encoding, _) = session.command("replace_raw", Some(&utf16(CHANGED, false)));
	assert_eq!(
		changed_encoding["result"]["payload"]["code"],
		"raw_encoding_mismatch"
	);

	assert_eq!(session.document["revision"], "0");
	assert_eq!(session.command("raw", None).1.unwrap(), opening);
}

#[test]
fn utf16_structured_revert_splices_native_offsets_and_keeps_other_text() {
	for little in [true, false] {
		let opening = utf16(ORIGINAL, little);
		let mut session = Session::open(&opening);
		let section = &session.document["result"]["payload"]["sections"][0];
		let section_id = section["id"].clone();
		let value_id = section["keys"][0]["values"][0]["id"].clone();
		let (edited, _) = session.request(
			json!({"kind":"set_values","payload":{"edits":[{"id":value_id,"value":"008"}]}}),
			None,
		);
		assert_eq!(edited["result"]["kind"], "snapshot", "{edited}");
		assert_eq!(
			session.command("prepare", None).1.unwrap(),
			utf16(CHANGED, little)
		);
		let (reverted, _) = session.request(
			json!({"kind":"revert","payload":{"kind":"section","id":section_id}}),
			None,
		);
		assert_eq!(reverted["result"]["kind"], "snapshot", "{reverted}");
		assert_eq!(reverted["result"]["payload"]["dirty"], false);
		assert_eq!(session.command("prepare", None).1.unwrap(), opening);
	}
}
