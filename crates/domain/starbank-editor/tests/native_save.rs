use serde_json::{Value, json};
use starbank_core::Revision;
use starbank_editor::{EditorError, Engine, ExportSigning, NativeSaveReceipt, PROTOCOL_VERSION};
use starbank_signature::{SignatureStatus, SigningContext};

const ORIGINAL: &[u8] = b"<Bank/>";
const EDITED: &[u8] =
	br#"<Bank><Section name="Stats"><Key name="Count"><Value int="9"/></Key></Section></Bank>"#;
const LATER: &[u8] =
	br#"<Bank><Section name="Stats"><Key name="Count"><Value int="10"/></Key></Section></Bank>"#;

struct Bank {
	engine: Engine,
	id: String,
	revision: Revision,
}

impl Bank {
	fn open() -> Self {
		let mut engine = Engine::default();
		let opened = open_bank(&mut engine, ORIGINAL);
		Self {
			engine,
			id: opened["document_id"]
				.as_str()
				.expect("document ID")
				.to_owned(),
			revision: revision(&opened),
		}
	}

	fn command(&mut self, kind: &str, bytes: Option<&[u8]>) -> (Value, Option<Vec<u8>>) {
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "session",
			"document_id": self.id,
			"expected_revision": self.revision.to_string(),
			"command": {"kind": kind},
		});
		let reply = self.engine.dispatch(&control.to_string(), bytes);
		let response = serde_json::from_str(&reply.json).expect("structured wire reply");
		(response, reply.bytes)
	}

	fn change(&mut self, bytes: &[u8]) {
		let (response, _) = self.command("replace_raw", Some(bytes));
		assert_eq!(response["result"]["kind"], "snapshot", "{response}");
		self.revision = revision(&response);
	}

	fn prepare(&self, signing: ExportSigning<'_>) -> NativeSaveReceipt {
		self.engine
			.prepare_native_active_save(&self.id, self.revision, signing)
			.expect("active save receipt")
	}

	fn is_dirty(&mut self) -> bool {
		let (response, _) = self.command("snapshot", None);
		assert_eq!(response["result"]["kind"], "snapshot", "{response}");
		response["result"]["payload"]["dirty"]
			.as_bool()
			.expect("dirty status")
	}

	fn raw(&mut self) -> Vec<u8> {
		let (response, bytes) = self.command("raw", None);
		assert_eq!(response["result"]["kind"], "raw", "{response}");
		bytes.expect("raw XML attachment")
	}
}

fn open_bank(engine: &mut Engine, bytes: &[u8]) -> Value {
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind": "open", "payload": {"discard_current": true}},
	});
	let reply = engine.dispatch(&control.to_string(), Some(bytes));
	let response: Value = serde_json::from_str(&reply.json).expect("structured wire reply");
	assert_eq!(response["result"]["kind"], "snapshot", "{response}");
	response
}

fn revision(response: &Value) -> Revision {
	response["revision"]
		.as_str()
		.expect("revision string")
		.parse()
		.expect("canonical revision")
}

#[test]
fn native_receipt_is_bound_to_exact_bytes_and_requires_verified_acknowledgement() {
	let mut bank = Bank::open();
	bank.change(EDITED);
	let context =
		SigningContext::new("1-S2-1-111", "1-S2-1-222", "Stats").expect("signing context");
	let rejected = bank.prepare(ExportSigning::Sign(&context));
	assert_eq!(rejected.document_id(), bank.id);
	assert_eq!(rejected.revision(), bank.revision);
	assert_eq!(rejected.signature_status(), SignatureStatus::Valid);
	assert_ne!(rejected.bytes(), EDITED);
	bank.engine
		.validate_native_active_save(&rejected)
		.expect("current receipt");
	drop(rejected);
	assert!(bank.is_dirty());
	assert_eq!(bank.raw(), EDITED);

	let committed = bank.prepare(ExportSigning::Sign(&context));
	let expected = committed.bytes().to_vec();
	bank.engine
		.validate_native_active_save(&committed)
		.expect("current receipt");
	let acknowledged = bank
		.engine
		.acknowledge_verified_native_active_save(committed)
		.expect("verified write acknowledges");
	assert_eq!(Some(acknowledged), bank.revision.checked_next());
	bank.revision = acknowledged;
	assert!(!bank.is_dirty());
	assert_eq!(bank.raw(), expected);
}

#[test]
fn stale_receipt_cannot_start_a_write_but_verified_earlier_bytes_remain_the_checkpoint() {
	let mut bank = Bank::open();
	bank.change(EDITED);
	let receipt = bank.prepare(ExportSigning::Preserve);
	let written_revision = bank.revision;
	assert_eq!(receipt.bytes(), EDITED);
	bank.engine
		.validate_native_active_save(&receipt)
		.expect("initially current");
	bank.change(LATER);

	assert!(matches!(
		bank.engine
			.prepare_native_active_save(&bank.id, written_revision, ExportSigning::Preserve),
		Err(EditorError::StaleRevision { .. })
	));
	assert!(matches!(
		bank.engine.validate_native_active_save(&receipt),
		Err(EditorError::StaleRevision { expected, current })
			if expected == written_revision && current == bank.revision
	));
	assert_eq!(
		bank.engine
			.acknowledge_verified_native_active_save(receipt)
			.expect("earlier write was verified"),
		bank.revision
	);
	assert!(bank.is_dirty());
	assert_eq!(bank.raw(), LATER);
	let (undone, _) = bank.command("undo", None);
	bank.revision = revision(&undone);
	assert_eq!(bank.raw(), EDITED);
	assert!(!bank.is_dirty());
}

#[test]
fn the_revert_wire_command_restores_the_verified_checkpoint_and_dirty_projection() {
	let mut bank = Bank::open();
	bank.change(EDITED);
	let (snapshot, _) = bank.command("snapshot", None);
	let saved_id = snapshot["result"]["payload"]["sections"][0]["id"].clone();
	assert_eq!(snapshot["result"]["payload"]["dirty_section_count"], 1);
	assert_eq!(snapshot["result"]["payload"]["sections"][0]["dirty"], true);
	let receipt = bank.prepare(ExportSigning::Preserve);
	bank.revision = bank
		.engine
		.acknowledge_verified_native_active_save(receipt)
		.unwrap();

	bank.change(LATER);
	let latest = bank.revision;
	bank.revision = Revision::INITIAL;
	let (stale, _) = bank.command("revert_to_saved", None);
	assert_eq!(stale["result"]["payload"]["code"], "stale_revision");
	bank.revision = latest;
	let (attached, _) = bank.command("revert_to_saved", Some(b"unexpected"));
	assert_eq!(
		attached["result"]["payload"]["code"],
		"unexpected_attachment"
	);
	assert_eq!(bank.raw(), LATER);

	let (restored, _) = bank.command("revert_to_saved", None);
	bank.revision = revision(&restored);
	assert_eq!(Some(bank.revision), latest.checked_next());
	let payload = &restored["result"]["payload"];
	assert_eq!(payload["dirty"], false);
	assert_eq!(payload["dirty_section_count"], 0);
	assert_eq!(payload["sections"][0]["id"], saved_id);
	assert_eq!(payload["sections"][0]["dirty"], false);
	assert_eq!(bank.raw(), EDITED);
	assert_eq!(bank.command("opening_raw", None).1.unwrap(), ORIGINAL);
	let (reset, _) = bank.command("reset", None);
	bank.revision = revision(&reset);
	let payload = &reset["result"]["payload"];
	assert_eq!(payload["dirty"], true);
	assert_eq!(payload["dirty_section_count"], 1);
	assert_eq!(payload["sections"], json!([]));
	assert_eq!(bank.raw(), ORIGINAL);
}

#[test]
fn replaced_and_foreign_sessions_reject_receipts_even_when_public_ids_match() {
	let mut first = Bank::open();
	let mut second = Bank::open();
	first.change(EDITED);
	second.change(LATER);
	assert_eq!(first.id, second.id);
	assert_eq!(first.revision, second.revision);
	let foreign = first.prepare(ExportSigning::Preserve);
	assert!(matches!(
		second.engine.validate_native_active_save(&foreign),
		Err(EditorError::WrongSession)
	));
	assert!(matches!(
		second
			.engine
			.acknowledge_verified_native_active_save(foreign),
		Err(EditorError::WrongSession)
	));
	assert!(second.is_dirty());
	assert_eq!(second.raw(), LATER);

	let expired = first.prepare(ExportSigning::Preserve);
	let opened = open_bank(&mut first.engine, ORIGINAL);
	first.id = opened["document_id"]
		.as_str()
		.expect("new document ID")
		.to_owned();
	first.revision = revision(&opened);
	assert!(matches!(
		first.engine.validate_native_active_save(&expired),
		Err(EditorError::WrongSession)
	));
	assert!(matches!(
		first
			.engine
			.acknowledge_verified_native_active_save(expired),
		Err(EditorError::WrongSession)
	));
	assert!(!first.is_dirty());
	assert_eq!(first.raw(), ORIGINAL);
}

#[test]
fn a_newer_verified_receipt_invalidates_older_preparation_at_the_same_revision() {
	let mut bank = Bank::open();
	bank.change(EDITED);
	let earlier = bank.prepare(ExportSigning::Preserve);
	let later = bank.prepare(ExportSigning::Preserve);
	assert_eq!(earlier.revision(), later.revision());
	assert_eq!(
		bank.engine
			.acknowledge_verified_native_active_save(later)
			.expect("latest verified write"),
		bank.revision
	);
	assert!(matches!(
		bank.engine.validate_native_active_save(&earlier),
		Err(EditorError::StaleSaveAcknowledgement)
	));
	assert!(matches!(
		bank.engine.acknowledge_verified_native_active_save(earlier),
		Err(EditorError::StaleSaveAcknowledgement)
	));
	assert!(!bank.is_dirty());
}

#[test]
fn json_commands_can_prepare_an_export_without_recording_a_native_save() {
	let mut bank = Bank::open();
	bank.change(EDITED);
	let (prepared, bytes) = bank.command("prepare", None);
	assert_eq!(prepared["result"]["kind"], "prepared", "{prepared}");
	assert_eq!(bytes.as_deref(), Some(EDITED));
	let (rejected, _) = bank.command("acknowledge_verified_native_active_save", None);
	assert_eq!(rejected["result"]["payload"]["code"], "invalid_request");
	assert!(bank.is_dirty());
}
