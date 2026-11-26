use sc2bank::Limits;
use starbank_editor::{EditorError, Session, ValueChange};
use starbank_testkit::synthetic_bank;

const RETAINED_UNDO_STEPS: usize = 128;

#[test]
fn undo_window_discards_oldest_step_and_preserves_complete_retained_edits()
-> Result<(), EditorError> {
	let bank = synthetic_bank(1);
	let mut session = Session::open(&bank, Limits::default(), None)?;
	let first_value = session.document().sections()[0].keys()[0].values()[0].id();
	let id = session.id_of_value(first_value).expect("projected value");
	let mut revision = session.revision();

	for number in 1..=RETAINED_UNDO_STEPS + 1 {
		let value = number.to_string();
		revision = session.set_values(revision, &[ValueChange { id, value: &value }])?;
	}
	for _ in 0..RETAINED_UNDO_STEPS {
		revision = session.undo(revision)?;
	}
	assert!(!session.can_undo());
	assert!(session.can_redo());
	assert!(matches!(
		session.undo(revision),
		Err(EditorError::NoHistory)
	));
	assert_eq!(session.revision(), revision);
	assert_eq!(
		session.document().sections()[0].keys()[0].values()[0]
			.attribute()
			.expect("value remains an integer")
			.value(),
		"1",
	);
	Ok(())
}
