use sc2bank::{Limits, StructuralEditError, ValueKind};
use starbank_core::Revision;
use starbank_editor::{EditorError, LexemeError, Session, StructureChange, ValueChange};

const DUPLICATE_BANK: &[u8] = br#"<Bank><Section name='S'><Key name='K'><Value int='1'/></Key></Section><!-- keep --><Section name='S'><Key name='K'><Value int='2'/></Key></Section></Bank>"#;

#[test]
fn removal_maps_surviving_session_ids_across_changed_format_ids_and_history() {
	let mut session = Session::open(DUPLICATE_BANK, Limits::default(), None).unwrap();
	let sections = session.document().sections();
	let removed_section = session.id_of_section(sections[0].id()).unwrap();
	let removed_value = session
		.id_of_value(sections[0].keys()[0].values()[0].id())
		.unwrap();
	let surviving_section = session.id_of_section(sections[1].id()).unwrap();
	let surviving_key = session.id_of_key(sections[1].keys()[0].id()).unwrap();
	let old_format_id = sections[1].keys()[0].values()[0].id();
	let surviving_value = session.id_of_value(old_format_id).unwrap();

	let removed = session
		.apply_structure(
			Revision::INITIAL,
			StructureChange::RemoveSection {
				id: removed_section,
			},
		)
		.unwrap();
	assert_eq!(removed.added, None);
	assert_eq!(removed.revision, Revision::INITIAL.checked_next().unwrap());
	assert_eq!(
		session.document().source(),
		br#"<Bank><!-- keep --><Section name='S'><Key name='K'><Value int='2'/></Key></Section></Bank>"#,
	);
	let section = &session.document().sections()[0];
	let key = &section.keys()[0];
	let value = &key.values()[0];
	assert_ne!(value.id(), old_format_id);
	assert_eq!(session.id_of_section(section.id()), Some(surviving_section));
	assert_eq!(session.id_of_key(key.id()), Some(surviving_key));
	assert_eq!(session.id_of_value(value.id()), Some(surviving_value));
	assert!(matches!(
		session.set_values(removed.revision, &[ValueChange { id: removed_value, value: "99" }]),
		Err(EditorError::UnknownValue(id)) if id == removed_value,
	));

	let undone = session.undo(removed.revision).unwrap();
	assert_eq!(session.document().source(), DUPLICATE_BANK);
	let sections = session.document().sections();
	assert_eq!(
		session.id_of_section(sections[0].id()),
		Some(removed_section)
	);
	assert_eq!(
		session.id_of_value(sections[0].keys()[0].values()[0].id()),
		Some(removed_value),
	);
	let redone = session.redo(undone).unwrap();
	let stable_value = session.document().sections()[0].keys()[0].values()[0].id();
	assert_eq!(session.id_of_value(stable_value), Some(surviving_value));
	let final_revision = session
		.set_values(
			redone,
			&[ValueChange {
				id: surviving_value,
				value: "3",
			}],
		)
		.unwrap();
	assert_eq!(final_revision, redone.checked_next().unwrap());
	assert_eq!(
		session.document().sections()[0].keys()[0].values()[0]
			.attribute()
			.unwrap()
			.value(),
		"3",
	);
}

#[test]
fn adds_allocate_monotonic_ids_even_after_undo_and_branching() {
	let mut session = Session::open(b"<Bank/>", Limits::default(), None).unwrap();
	let section = session
		.apply_structure(
			Revision::INITIAL,
			StructureChange::AddSection { name: "New" },
		)
		.unwrap();
	let section_id = section.added.unwrap();
	let key = session
		.apply_structure(
			section.revision,
			StructureChange::AddKey {
				section: section_id,
				name: "Counter",
			},
		)
		.unwrap();
	let key_id = key.added.unwrap();
	assert!(key_id > section_id);
	let value = session
		.apply_structure(
			key.revision,
			StructureChange::AddValue {
				key: key_id,
				name: "Count",
				kind: &ValueKind::Int,
				value: "9,007,199,254,740,993",
			},
		)
		.unwrap();
	let value_id = value.added.unwrap();
	assert!(value_id > key_id);
	let current = &session.document().sections()[0].keys()[0].values()[0];
	assert_eq!(session.id_of_value(current.id()), Some(value_id));
	assert_eq!(current.attribute().unwrap().value(), "9007199254740993");

	let undone = session.undo(value.revision).unwrap();
	assert!(session.can_redo());
	assert!(
		session.document().sections()[0].keys()[0]
			.values()
			.is_empty()
	);
	let redone = session.redo(undone).unwrap();
	let restored = &session.document().sections()[0].keys()[0].values()[0];
	assert_eq!(session.id_of_value(restored.id()), Some(value_id));

	let undone_again = session.undo(redone).unwrap();
	let replacement = session
		.apply_structure(
			undone_again,
			StructureChange::AddValue {
				key: key_id,
				name: "Count",
				kind: &ValueKind::Int,
				value: "0",
			},
		)
		.unwrap();
	assert!(replacement.added.unwrap() > value_id);
	assert!(!session.can_redo());
}

#[test]
fn renames_and_conversion_keep_the_selected_occurrence_and_unrelated_xml() {
	let original = br#"<Bank><Section name='A'><Key name='B'><Old string = '&#x31;'></Old ></Key></Section><Signature value='unchanged'/></Bank>"#;
	let mut session = Session::open(original, Limits::default(), None).unwrap();
	let section = &session.document().sections()[0];
	let key = &section.keys()[0];
	let value = &key.values()[0];
	let section_id = session.id_of_section(section.id()).unwrap();
	let key_id = session.id_of_key(key.id()).unwrap();
	let value_id = session.id_of_value(value.id()).unwrap();

	for change in [
		StructureChange::RenameSection {
			id: section_id,
			name: "C & D",
		},
		StructureChange::RenameKey {
			id: key_id,
			name: "E",
		},
		StructureChange::RenameValue {
			id: value_id,
			name: "New",
		},
		StructureChange::SetValueType {
			id: value_id,
			kind: &ValueKind::Int,
			value: "1",
		},
	] {
		let expected = session.revision().checked_next().unwrap();
		let outcome = session.apply_structure(session.revision(), change).unwrap();
		assert_eq!(outcome.revision, expected);
		assert_eq!(outcome.added, None);
	}
	assert_eq!(
		session.document().source(),
		br#"<Bank><Section name='C &amp; D'><Key name='E'><New int = '&#x31;'></New ></Key></Section><Signature value='unchanged'/></Bank>"#,
	);
	let current = &session.document().sections()[0].keys()[0].values()[0];
	assert_eq!(session.id_of_value(current.id()), Some(value_id));

	let revision = session.revision();
	let result = session
		.apply_structure(
			revision,
			StructureChange::SetValueType {
				id: value_id,
				kind: &ValueKind::Int,
				value: "1",
			},
		)
		.unwrap();
	assert_eq!(result.revision, revision);
	let restored = session.undo(revision).unwrap();
	assert_eq!(restored, revision.checked_next().unwrap());
	assert_eq!(
		session.document().source(),
		br#"<Bank><Section name='C &amp; D'><Key name='E'><New string = '&#x31;'></New ></Key></Section><Signature value='unchanged'/></Bank>"#,
	);
}

#[test]
fn failed_and_noop_structure_changes_preserve_revision_and_redo() {
	let mut session = Session::open(DUPLICATE_BANK, Limits::default(), None).unwrap();
	let section = &session.document().sections()[0];
	let key = &section.keys()[0];
	let section_id = session.id_of_section(section.id()).unwrap();
	let key_id = session.id_of_key(key.id()).unwrap();
	let value_id = session.id_of_value(key.values()[0].id()).unwrap();

	let renamed = session
		.apply_structure(
			Revision::INITIAL,
			StructureChange::RenameSection {
				id: section_id,
				name: "Temporary",
			},
		)
		.unwrap();
	let revision = session.undo(renamed.revision).unwrap();
	assert!(session.can_redo());
	assert!(matches!(
		session.apply_structure(
			Revision::INITIAL,
			StructureChange::RemoveSection { id: section_id }
		),
		Err(EditorError::StaleRevision { .. }),
	));
	assert!(matches!(
		session.apply_structure(revision, StructureChange::RemoveSection { id: value_id }),
		Err(EditorError::UnknownSection(id)) if id == value_id,
	));
	assert!(matches!(
		session.apply_structure(
			revision,
			StructureChange::AddValue { key: key_id, name: "Value", kind: &ValueKind::Fixed, value: "12,34" },
		),
		Err(EditorError::InvalidAddedValue { key, source: LexemeError::InvalidFixed }) if key == key_id,
	));
	assert!(matches!(
		session.apply_structure(
			revision,
			StructureChange::SetValueType { id: value_id, kind: &ValueKind::Int, value: "1e9" },
		),
		Err(EditorError::InvalidValue { id, source: LexemeError::InvalidInteger }) if id == value_id,
	));
	assert!(matches!(
		session.apply_structure(
			revision,
			StructureChange::RenameValue {
				id: value_id,
				name: "bad name"
			}
		),
		Err(EditorError::Structure(
			StructuralEditError::InvalidElementName
		)),
	));
	let no_op = session
		.apply_structure(
			revision,
			StructureChange::RenameSection {
				id: section_id,
				name: "S",
			},
		)
		.unwrap();
	assert_eq!(no_op.revision, revision);
	assert_eq!(no_op.added, None);
	assert_eq!(session.document().source(), DUPLICATE_BANK);
	assert!(!session.is_dirty());
	assert!(session.can_redo());
}
