use sc2bank::Limits;
use starbank_core::Revision;
use starbank_editor::{
	EditorError, ExportSigning, OccurrenceId, Session, StructureChange, ValueChange,
};
use starbank_signature::{SignatureStatus, SigningContext};

const DUPLICATE_SECTIONS: &[u8] = b"<Bank><Section name='Stats'><Key name='Total'><Value int='001'/></Key></Section><Section name='Stats'><Key name='Total'><Value int='002'/></Key></Section></Bank>";

fn section_ids(session: &Session) -> Vec<OccurrenceId> {
	session
		.document()
		.sections()
		.iter()
		.map(|section| session.id_of_section(section.id()).unwrap())
		.collect()
}

fn first_value_id(session: &Session) -> OccurrenceId {
	let value = &session.document().sections()[0].keys()[0].values()[0];
	session.id_of_value(value.id()).unwrap()
}

#[test]
fn dirty_sections_follow_the_saved_occurrences_through_structure_and_history() {
	let mut session = Session::open(DUPLICATE_SECTIONS, Limits::default(), None).unwrap();
	let opening_ids = section_ids(&session);
	let value_id = first_value_id(&session);
	assert_eq!(session.dirty_sections().unwrap().count(), 0);

	let edited = session
		.set_values(
			Revision::INITIAL,
			&[ValueChange {
				id: value_id,
				value: "031",
			}],
		)
		.unwrap();
	let dirty = session.dirty_sections().unwrap();
	assert_eq!(dirty.count(), 1);
	assert!(dirty.contains(opening_ids[0]));
	assert!(!dirty.contains(opening_ids[1]));

	let receipt = session.prepare(edited, ExportSigning::Preserve).unwrap();
	let saved_bytes = receipt.bytes().to_vec();
	assert!(
		session.is_dirty(),
		"preparation alone never changes the baseline"
	);
	let deleted = session
		.apply_structure(
			edited,
			StructureChange::RemoveSection { id: opening_ids[1] },
		)
		.unwrap();
	let inserted = session
		.apply_structure(
			deleted.revision,
			StructureChange::AddSection { name: "New Stats" },
		)
		.unwrap();
	let added = inserted.added.unwrap();
	assert_eq!(session.dirty_sections().unwrap().count(), 3);
	assert_eq!(
		session.acknowledge_verified_active_save(receipt).unwrap(),
		inserted.revision
	);
	let dirty = session.dirty_sections().unwrap();
	assert_eq!(dirty.count(), 2, "one deletion and one addition");
	assert!(dirty.contains(added));
	assert!(!dirty.contains(opening_ids[0]));

	let working = session.document().source().to_vec();
	assert!(matches!(
		session.revert_to_saved(edited),
		Err(EditorError::StaleRevision { .. })
	));
	assert_eq!(session.document().source(), working);
	let reverted = session.revert_to_saved(inserted.revision).unwrap();
	assert_eq!(section_ids(&session), opening_ids);
	assert_eq!(session.document().source(), saved_bytes);
	assert_eq!(session.dirty_sections().unwrap().count(), 0);
	assert!(!session.is_dirty());
	assert_eq!(session.revert_to_saved(reverted).unwrap(), reverted);

	let undone = session.undo(reverted).unwrap();
	assert_eq!(session.document().source(), working);
	assert_eq!(section_ids(&session), [opening_ids[0], added]);
	assert_eq!(session.dirty_sections().unwrap().count(), 2);
	let redone = session.redo(undone).unwrap();
	assert_eq!(session.document().source(), saved_bytes);
	assert!(!session.is_dirty());

	let reset = session.reset(redone).unwrap();
	assert_eq!(session.document().source(), DUPLICATE_SECTIONS);
	assert!(session.is_dirty());
	assert_eq!(session.dirty_sections().unwrap().count(), 1);
	assert!(!session.opening_diff().unwrap().source_changed);
	assert!(session.revert_to_saved(reset).unwrap() > reset);
	assert_eq!(session.document().source(), saved_bytes);
	assert!(session.opening_diff().unwrap().source_changed);
}

#[test]
fn raw_document_only_changes_leave_byte_identical_duplicate_sections_clean() {
	let text = "<Bank><Section name='Same'/><Section name='Same'/></Bank>";
	let mut session = Session::open(text.as_bytes(), Limits::default(), None).unwrap();
	let opening_ids = section_ids(&session);
	let changed = format!("{text}<!-- note -->");
	let raw_revision = session
		.replace_raw(Revision::INITIAL, changed.as_bytes())
		.unwrap();
	assert!(raw_revision > Revision::INITIAL);
	assert_ne!(section_ids(&session), opening_ids);
	assert!(session.is_dirty());
	let dirty = session.dirty_sections().unwrap();
	assert_eq!(dirty.count(), 0);
	assert!(section_ids(&session).iter().all(|&id| !dirty.contains(id)));
}

#[test]
fn reverting_byte_identical_raw_xml_restores_checkpoint_ids_and_undoes_cleanly() {
	let mut session = Session::open(DUPLICATE_SECTIONS, Limits::default(), None).unwrap();
	let opening_ids = section_ids(&session);
	let value_id = first_value_id(&session);
	let edited = session
		.set_values(
			Revision::INITIAL,
			&[ValueChange {
				id: value_id,
				value: "031",
			}],
		)
		.unwrap();
	let pending = session.prepare(edited, ExportSigning::Preserve).unwrap();
	let saved = pending.bytes().to_vec();
	assert_eq!(
		session.acknowledge_verified_active_save(pending).unwrap(),
		edited
	);

	let away = session.replace_raw(edited, b"<Bank/>").unwrap();
	let same_saved = session.replace_raw(away, &saved).unwrap();
	let raw_ids = section_ids(&session);
	assert_ne!(raw_ids, opening_ids);
	assert!(!session.is_dirty());

	let reverted = session.revert_to_saved(same_saved).unwrap();
	assert!(reverted > same_saved);
	assert_eq!(session.document().source(), saved);
	assert_eq!(section_ids(&session), opening_ids);
	assert_eq!(first_value_id(&session), value_id);
	assert_eq!(session.revert_to_saved(reverted).unwrap(), reverted);
	assert!(!session.is_dirty());

	let undone = session.undo(reverted).unwrap();
	assert_eq!(section_ids(&session), raw_ids);
	assert_eq!(session.document().source(), saved);
	let redone = session.redo(undone).unwrap();
	assert_eq!(section_ids(&session), opening_ids);
	assert_eq!(session.document().source(), saved);

	let raw_opening = session.replace_raw(redone, DUPLICATE_SECTIONS).unwrap();
	assert_ne!(section_ids(&session), opening_ids);
	let reset = session.reset(raw_opening).unwrap();
	assert!(reset > raw_opening);
	assert_eq!(section_ids(&session), opening_ids);
	assert_eq!(session.document().source(), DUPLICATE_SECTIONS);
	assert!(session.is_dirty());
	assert_eq!(session.dirty_sections().unwrap().count(), 1);
	let edited_again = session
		.set_values(
			reset,
			&[ValueChange {
				id: value_id,
				value: "041",
			}],
		)
		.unwrap();
	assert!(
		session
			.revert_section(edited_again, opening_ids[0])
			.unwrap()
			> edited_again
	);
	assert_eq!(session.document().source(), DUPLICATE_SECTIONS);
}

#[test]
fn reverting_signed_utf16_restores_exact_bytes_even_after_a_later_raw_edit() {
	let xml = "<?xml version='1.0' encoding='UTF-16'?>\r\n<Bank><Section name='星'><Key name='K'><Value int='001'/></Key></Section></Bank>\r\n";
	let opening: Vec<_> = std::iter::once(0xfeff)
		.chain(xml.encode_utf16())
		.flat_map(u16::to_be_bytes)
		.collect();
	let mut session = Session::open(&opening, Limits::default(), None).unwrap();
	let initial_ids = section_ids(&session);
	let value_id = first_value_id(&session);
	let edited = session
		.set_values(
			Revision::INITIAL,
			&[ValueChange {
				id: value_id,
				value: "031",
			}],
		)
		.unwrap();
	let context = SigningContext::new("1-S2-1-111", "1-S2-1-222", "Stats").unwrap();
	let signed = session
		.prepare(edited, ExportSigning::Sign(&context))
		.unwrap();
	let saved_bytes = signed.bytes().to_vec();
	let current_bytes = session
		.document()
		.encode_xml_text(
			&format!("{}<!-- later -->", session.document().xml_text()),
			Limits::default(),
		)
		.unwrap();
	let raw = session.replace_raw(edited, &current_bytes).unwrap();
	let raw_ids = section_ids(&session);
	assert_ne!(raw_ids, initial_ids);
	assert_eq!(
		session.acknowledge_verified_active_save(signed).unwrap(),
		raw
	);
	assert_eq!(session.document().source(), current_bytes);
	assert!(session.is_dirty());
	assert_eq!(session.dirty_sections().unwrap().count(), 0);

	let reverted = session.revert_to_saved(raw).unwrap();
	assert_eq!(session.document().source(), saved_bytes);
	assert_eq!(section_ids(&session), initial_ids);
	assert_eq!(first_value_id(&session), value_id);
	assert_eq!(session.signature_status(), SignatureStatus::Valid);
	assert!(!session.is_dirty());
	let undone = session.undo(reverted).unwrap();
	assert_eq!(session.document().source(), current_bytes);
	assert_eq!(section_ids(&session), raw_ids);
	assert!(session.is_dirty());
	assert!(session.reset(undone).unwrap() > undone);
	assert_eq!(session.document().source(), opening);
}
