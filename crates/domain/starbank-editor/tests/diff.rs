use sc2bank::{Limits, ValueKind};
use starbank_core::Revision;
use starbank_editor::{
	EditorError, ExportSigning, OccurrenceChange, OccurrenceId, OccurrenceKind, Session,
	StructureChange, ValueChange,
};
use starbank_signature::SigningContext;

const OPENING: &[u8] = br#"<Bank version="1"><!-- retain -->
<Section name="S"><Key name='K'><Part int='&#x30;01'/><Other fixed="2.00"/></Key></Section>
<Section name="S"><Key name='K'><Part string='A, B'/></Key></Section>
</Bank>"#;

fn bank() -> Session {
	Session::open(OPENING, Limits::default(), None).unwrap()
}

fn section_id(session: &Session, section: usize) -> OccurrenceId {
	session
		.id_of_section(session.document().sections()[section].id())
		.unwrap()
}

fn value_id(session: &Session, section: usize, value: usize) -> OccurrenceId {
	session
		.id_of_value(session.document().sections()[section].keys()[0].values()[value].id())
		.unwrap()
}

#[test]
fn diff_uses_occurrence_identity_and_exact_decoded_values() {
	let mut session = bank();
	let edited = value_id(&session, 0, 0);
	let unchanged = value_id(&session, 1, 0);
	assert!(!session.opening_diff().unwrap().source_changed);

	let revision = session
		.set_values(
			Revision::INITIAL,
			&[ValueChange {
				id: edited,
				value: "9,007,199,254,740,993",
			}],
		)
		.unwrap();
	assert!(revision > Revision::INITIAL);

	let diff = session.opening_diff().unwrap();
	assert!(diff.source_changed);
	assert_eq!(diff.changes.len(), 3);
	assert_eq!(diff.changes[0].id, section_id(&session, 0));
	assert_eq!(diff.changes[0].kind, OccurrenceKind::Section);
	assert!(!diff.changes.iter().any(|change| change.id == unchanged));
	let value = &diff.changes[2];
	assert_eq!(value.id, edited);
	assert_eq!(value.kind, OccurrenceKind::Value);
	let OccurrenceChange::Changed { before, after } = &value.change else {
		panic!("edited value must have before and after states");
	};
	assert_eq!(before.name.as_deref(), Some("Part"));
	assert_eq!(before.parent, after.parent);
	assert_eq!(before.value.as_ref().unwrap().text, "001");
	assert_eq!(after.value.as_ref().unwrap().text, "9007199254740993");
	assert_eq!(after.value.as_ref().unwrap().kind, ValueKind::Int);
}

#[test]
fn targeted_revert_restores_original_spelling_and_preserves_unrelated_edits() {
	let mut session = bank();
	let first = value_id(&session, 0, 0);
	let second = value_id(&session, 0, 1);
	let third = value_id(&session, 1, 0);
	let section = section_id(&session, 0);
	let edit = session
		.set_values(
			Revision::INITIAL,
			&[
				ValueChange {
					id: first,
					value: "123",
				},
				ValueChange {
					id: second,
					value: "4.20",
				},
				ValueChange {
					id: third,
					value: "New",
				},
			],
		)
		.unwrap();

	assert!(matches!(
		session.revert_value(Revision::INITIAL, first),
		Err(EditorError::StaleRevision { .. })
	));
	assert!(matches!(
		session.revert_section(edit, first),
		Err(EditorError::RevertUnavailable(id)) if id == first
	));

	let restore_value = session.revert_value(edit, first).unwrap();
	let value_source = String::from_utf8_lossy(session.document().source());
	assert!(value_source.contains("<Part int='&#x30;01'/>"));
	assert!(value_source.contains("<Other fixed=\"4.20\"/>"));

	let restore_section = session.revert_section(restore_value, section).unwrap();
	let expected = String::from_utf8_lossy(OPENING).replace("string='A, B'", "string='New'");
	assert_eq!(session.document().source(), expected.as_bytes());
	assert_eq!(value_id(&session, 0, 0), first);
	assert_eq!(value_id(&session, 0, 1), second);
	assert_eq!(value_id(&session, 1, 0), third);
	assert_eq!(
		session.revert_section(restore_section, section).unwrap(),
		restore_section
	);

	let undo = session.undo(restore_section).unwrap();
	assert_ne!(session.document().source(), expected.as_bytes());
	assert_eq!(value_id(&session, 0, 1), second);
	assert!(session.redo(undo).unwrap() > undo);
	assert_eq!(session.document().source(), expected.as_bytes());
}

#[test]
fn raw_replacement_uses_additions_and_removals_without_guessing_correspondence() {
	let mut session = bank();
	let first = value_id(&session, 0, 0);
	let raw = br#"<Bank><Section name="S"><Key name="K"><Part int="5"/></Key></Section></Bank>"#;
	let revision = session.replace_raw(Revision::INITIAL, raw).unwrap();
	let current = value_id(&session, 0, 0);
	let diff = session.opening_diff().unwrap();
	assert!(diff.source_changed);
	assert!(
		diff.changes.iter().any(|item| {
			item.id == first && matches!(item.change, OccurrenceChange::Removed(_))
		})
	);
	assert!(
		diff.changes.iter().any(|item| {
			item.id == current && matches!(item.change, OccurrenceChange::Added(_))
		})
	);
	for target in [first, current] {
		assert!(matches!(
			session.revert_value(revision, target),
			Err(EditorError::RevertUnavailable(id)) if id == target
		));
	}
	assert_eq!(session.document().source(), raw);
	assert_eq!(session.revision(), revision);

	assert!(session.reset(revision).unwrap() > revision);
	assert!(!session.opening_diff().unwrap().source_changed);
	assert_eq!(value_id(&session, 0, 0), first);
}

#[test]
fn signature_only_changes_appear_in_the_source_comparison() {
	let mut session = bank();
	let context = SigningContext::new("1-S2-1-12", "1-S2-1-34", "DiffTest").unwrap();
	let prepared = session
		.prepare(Revision::INITIAL, ExportSigning::Sign(&context))
		.unwrap();
	assert!(session.acknowledge_verified_active_save(prepared).unwrap() > Revision::INITIAL);
	assert!(!session.is_dirty());
	let diff = session.opening_diff().unwrap();
	assert!(diff.source_changed);
	assert!(diff.changes.is_empty());
}

#[test]
fn section_revert_restores_removed_descendants_and_discards_added_descendants() {
	let mut session = bank();
	let section = section_id(&session, 0);
	let key = session
		.id_of_key(session.document().sections()[0].keys()[0].id())
		.unwrap();
	let removed = value_id(&session, 0, 0);
	let surviving = value_id(&session, 0, 1);
	let other_section = section_id(&session, 1);
	let other_value = value_id(&session, 1, 0);

	let removal = session
		.apply_structure(
			Revision::INITIAL,
			StructureChange::RemoveValue { id: removed },
		)
		.unwrap();
	let addition = session
		.apply_structure(
			removal.revision,
			StructureChange::AddValue {
				key,
				name: "New",
				kind: &ValueKind::String,
				value: "Temporary",
			},
		)
		.unwrap();
	let added = addition.added.unwrap();
	let changed = session
		.set_values(
			addition.revision,
			&[ValueChange {
				id: other_value,
				value: "Changed",
			}],
		)
		.unwrap();
	let diff = session.opening_diff().unwrap();
	assert!(
		diff.changes.iter().any(|item| {
			item.id == removed && matches!(item.change, OccurrenceChange::Removed(_))
		})
	);
	assert!(
		diff.changes
			.iter()
			.any(|item| { item.id == added && matches!(item.change, OccurrenceChange::Added(_)) })
	);

	let reverted = session.revert_section(changed, section).unwrap();
	let expected = String::from_utf8_lossy(OPENING).replace("string='A, B'", "string='Changed'");
	assert_eq!(session.document().source(), expected.as_bytes());
	assert_eq!(section_id(&session, 0), section);
	assert_eq!(section_id(&session, 1), other_section);
	assert_eq!(value_id(&session, 0, 0), removed);
	assert_eq!(value_id(&session, 0, 1), surviving);
	assert_eq!(value_id(&session, 1, 0), other_value);
	let undo = session.undo(reverted).unwrap();
	assert_eq!(value_id(&session, 0, 0), surviving);
	assert_eq!(value_id(&session, 0, 1), added);
	assert!(session.redo(undo).unwrap() > undo);
	assert_eq!(session.document().source(), expected.as_bytes());
}
