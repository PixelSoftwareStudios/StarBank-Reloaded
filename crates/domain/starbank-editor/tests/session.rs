use sc2bank::Limits;
use starbank_core::Revision;
use starbank_editor::{EditorError, ExportSigning, OccurrenceId, Session, ValueChange};
use starbank_signature::{SignatureStatus, SigningContext};

const BANK: &[u8] = b"<?xml version='1.0'?>\r\n<Bank version='1'><Section name='P'><Key name='K'><A int='7'/><B fixed='2.00'/><C string='A, B'/></Key><Key name='K'><A int='4'/></Key></Section></Bank>";

fn open() -> Session {
	Session::open(BANK, Limits::default(), None).unwrap()
}

fn values(session: &Session) -> Vec<OccurrenceId> {
	session.document().sections()[0]
		.keys()
		.iter()
		.flat_map(|key| key.values())
		.map(|value| session.id_of_value(value.id()).unwrap())
		.collect()
}

fn contents(session: &Session) -> Vec<&str> {
	session.document().sections()[0]
		.keys()
		.iter()
		.flat_map(|key| key.values())
		.map(|value| value.attribute().unwrap().value())
		.collect()
}

#[test]
fn exact_multi_edit_is_atomic_and_undoes_in_one_step() {
	let mut session = open();
	let ids = values(&session);
	let next = session
		.set_values(
			Revision::INITIAL,
			&[
				ValueChange {
					id: ids[0],
					value: "9,007,199,254,740,993",
				},
				ValueChange {
					id: ids[1],
					value: "-1,234.500",
				},
				ValueChange {
					id: ids[2],
					value: "A, B, C",
				},
			],
		)
		.unwrap();
	assert_eq!(
		contents(&session),
		["9007199254740993", "-1234.500", "A, B, C", "4"]
	);
	assert_eq!(values(&session), ids);
	assert!(session.is_dirty());
	assert!(matches!(
		session.set_values(Revision::INITIAL, &[]),
		Err(EditorError::StaleRevision { .. })
	));
	let undo = session.undo(next).unwrap();
	assert_eq!(session.document().source(), BANK);
	assert!(!session.is_dirty());
	assert_eq!(values(&session), ids);
	assert!(session.redo(undo).unwrap() > undo);
	assert_eq!(contents(&session)[0], "9007199254740993");
}

#[test]
fn invalid_or_duplicate_value_leaves_document_and_history_intact() {
	let mut session = open();
	let ids = values(&session);
	let good = ValueChange {
		id: ids[0],
		value: "8",
	};
	let bad = ValueChange {
		id: ids[1],
		value: "12,34.0",
	};
	assert!(matches!(
		session.set_values(Revision::INITIAL, &[good, bad]),
		Err(EditorError::InvalidValue { .. })
	));
	assert!(matches!(
		session.set_values(Revision::INITIAL, &[good, good]),
		Err(EditorError::DuplicateValue(_))
	));
	assert_eq!(session.revision(), Revision::INITIAL);
	assert_eq!(session.document().source(), BANK);
	assert!(!session.can_undo());
}

#[test]
fn raw_replacement_rejects_drafts_and_allocates_non_reused_occurrences() {
	let mut session = open();
	let original = values(&session);
	assert!(matches!(
		session.replace_raw(Revision::INITIAL, b"<Bank>"),
		Err(EditorError::Parse(_))
	));
	assert_eq!(session.document().source(), BANK);
	let raw =
		b"<Bank><Section name='Q'><Key name='K'><Value string='changed'/></Key></Section></Bank>";
	let revised = session.replace_raw(Revision::INITIAL, raw).unwrap();
	assert_eq!(contents(&session), ["changed"]);
	assert!(!original.contains(&values(&session)[0]));
	assert!(matches!(
		session.set_values(
			revised,
			&[ValueChange {
				id: original[0],
				value: "1"
			}]
		),
		Err(EditorError::UnknownValue(_))
	));
	let reset = session.reset(revised).unwrap();
	assert_eq!(values(&session), original);
	assert_eq!(session.document().source(), BANK);
	assert!(session.undo(reset).unwrap() > reset);
	assert_eq!(session.document().source(), raw);
}

#[test]
fn preparation_has_no_effect_and_the_opening_checkpoint_survives_save() {
	let mut session = open();
	let revision = session
		.set_values(
			Revision::INITIAL,
			&[ValueChange {
				id: values(&session)[0],
				value: "123",
			}],
		)
		.unwrap();
	let prepared = session.prepare(revision, ExportSigning::Preserve).unwrap();
	assert!(session.is_dirty());
	assert_eq!(prepared.bytes(), session.document().source());
	assert_eq!(prepared.signature_status(), SignatureStatus::Unsigned);
	assert_eq!(
		session.acknowledge_verified_active_save(prepared).unwrap(),
		revision
	);
	assert!(!session.is_dirty());
	assert!(session.reset(revision).unwrap() > revision);
	assert!(session.is_dirty());
	assert_eq!(session.document().source(), BANK);
}

#[test]
fn verified_older_save_keeps_later_edits_dirty_and_rejects_other_sessions() {
	let mut session = open();
	let id = values(&session)[0];
	let first = session
		.set_values(Revision::INITIAL, &[ValueChange { id, value: "8" }])
		.unwrap();
	let prepared = session.prepare(first, ExportSigning::Preserve).unwrap();
	let second = session
		.set_values(first, &[ValueChange { id, value: "9" }])
		.unwrap();
	let foreign = open()
		.prepare(Revision::INITIAL, ExportSigning::Preserve)
		.unwrap();
	assert!(matches!(
		session.acknowledge_verified_active_save(foreign),
		Err(EditorError::WrongSession)
	));
	assert_eq!(
		session.acknowledge_verified_active_save(prepared).unwrap(),
		second
	);
	assert!(session.is_dirty());
	assert_eq!(contents(&session)[0], "9");
	assert!(session.undo(second).unwrap() > second);
	assert_eq!(contents(&session)[0], "8");
	assert!(!session.is_dirty());
}

#[test]
fn signing_preparation_uses_destination_context_and_only_acknowledgement_adopts_it() {
	let mut session = open();
	let ids = values(&session);
	let context = SigningContext::new("1-S2-1-100", "1-S2-1-200", "NewName").unwrap();
	let prepared = session
		.prepare(Revision::INITIAL, ExportSigning::Sign(&context))
		.unwrap();
	assert_eq!(prepared.signature_status(), SignatureStatus::Valid);
	assert_eq!(session.document().source(), BANK);
	assert_eq!(session.signature_status(), SignatureStatus::Unsigned);
	let signed = session.acknowledge_verified_active_save(prepared).unwrap();
	assert!(signed > Revision::INITIAL);
	assert_eq!(values(&session), ids);
	assert_eq!(session.signature_status(), SignatureStatus::Valid);
	assert!(!session.is_dirty());
}
