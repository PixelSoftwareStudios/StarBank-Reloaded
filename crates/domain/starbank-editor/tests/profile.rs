use sc2bank::Limits;
use starbank_core::Revision;
use starbank_editor::{
	EditorError, LexemeError, ProfileEdit, ProfileEvidence, ProfileSessionError, Session,
	ValueChange,
};
use starbank_profiles::{MatchOutcome, Profile, ProfileEditError, ProfileInput, ProfileRegistry};
use starbank_signature::SigningContext;

const DEFINITION: &str = include_str!("../../starbank-profiles/tests/fixtures/training.json");
const BANK: &[u8] = include_bytes!("../../starbank-profiles/tests/fixtures/training.sc2bank");
const PROFILE_ID: &str = "synthetic-training";

fn context() -> SigningContext {
	SigningContext::new("9-S2-1-123", "9-S2-1-456", "TrainingBank").unwrap()
}

fn registry() -> ProfileRegistry {
	ProfileRegistry::new(vec![Profile::from_json(DEFINITION).unwrap()]).unwrap()
}

fn edit<'a>(field_id: &'a str, input: ProfileInput<'a>) -> ProfileEdit<'a> {
	ProfileEdit {
		profile_id: PROFILE_ID,
		field_id,
		input,
		evidence: ProfileEvidence::default(),
	}
}

#[test]
fn grouped_profile_edits_commit_every_affected_value_as_one_undo_step() {
	let mut session = Session::open(BANK, Limits::default(), Some(context())).unwrap();
	let registry = registry();
	let score = edit("score", ProfileInput::Text("9,007,199,254,740,993.50"));
	let initial = session.revision();
	let preview = session
		.preview_profile_edit(initial, &registry, score)
		.unwrap();
	assert_eq!(
		preview
			.iter()
			.map(|entry| (entry.field_id.as_str(), entry.value.as_str()))
			.collect::<Vec<_>>(),
		[
			("reward", "54043195528445961.25"),
			("score", "9007199254740993.50"),
			("total", "18014398509481987.25"),
		]
	);
	assert_eq!(session.document().source(), BANK);
	assert!(!session.is_dirty());
	let changed = session
		.apply_profile_edit(initial, &registry, score)
		.unwrap();
	assert_eq!(changed, initial.checked_next().unwrap());
	assert!(session.is_dirty());
	let document = session.document();
	let changed_bytes = document.source().to_vec();
	for entry in preview {
		let node = document
			.sections()
			.iter()
			.flat_map(|section| section.keys())
			.flat_map(|key| key.values())
			.find(|node| session.id_of_value(node.id()) == Some(entry.id))
			.unwrap();
		assert_eq!(node.attribute().unwrap().value(), entry.value);
	}
	assert!(matches!(
		session.apply_profile_edit(initial, &registry, score),
		Err(EditorError::StaleRevision { expected, current }) if expected == initial && current == changed
	));

	let undone = session.undo(changed).unwrap();
	assert_eq!(session.document().source(), BANK);
	assert!(!session.can_undo());
	let redone = session.redo(undone).unwrap();
	assert_ne!(redone, changed);
	assert_eq!(session.document().source(), changed_bytes);
}

#[test]
fn explicit_checkbox_mapping_leaves_generic_flag_edits_unavailable() {
	let mut session = Session::open(BANK, Limits::default(), Some(context())).unwrap();
	let registry = registry();
	let checkbox = edit("enabled", ProfileInput::Checked(false));
	let revision = session.revision();
	let preview = session
		.preview_profile_edit(revision, &registry, checkbox)
		.unwrap();
	assert_eq!(preview.len(), 1);
	assert_eq!(preview[0].previous, "yes");
	assert_eq!(preview[0].value, "no");
	assert!(matches!(
		session.set_values(
			revision,
			&[ValueChange {
				id: preview[0].id,
				value: "no"
			}]
		),
		Err(EditorError::InvalidValue {
			source: LexemeError::UnsupportedKind,
			..
		})
	));
	let switched = session
		.apply_profile_edit(revision, &registry, checkbox)
		.unwrap();
	assert_eq!(
		session.document().source(),
		core::str::from_utf8(BANK)
			.unwrap()
			.replace("flag=\"yes\"", "flag=\"no\"")
			.as_bytes()
	);
	assert_eq!(
		session
			.apply_profile_edit(switched, &registry, checkbox)
			.unwrap(),
		switched
	);
	assert_eq!(session.revision(), switched);
}

#[test]
fn missing_context_wrong_profile_and_invalid_edits_advance_nothing() {
	let registry = registry();
	let score = edit("score", ProfileInput::Text("2.00"));
	let mut unknown = Session::open(BANK, Limits::default(), None).unwrap();
	assert!(matches!(
		unknown.match_profile(&registry, ProfileEvidence::default()),
		MatchOutcome::Unknown
	));
	assert!(matches!(
		unknown.apply_profile_edit(Revision::INITIAL, &registry, score),
		Err(EditorError::Profile(ProfileSessionError::Unknown))
	));
	let mut session = Session::open(BANK, Limits::default(), Some(context())).unwrap();
	let wrong_profile = ProfileEdit {
		profile_id: "unrelated-profile",
		..score
	};
	assert!(matches!(
		session.apply_profile_edit(Revision::INITIAL, &registry, wrong_profile),
		Err(EditorError::Profile(ProfileSessionError::ProfileChanged))
	));
	assert!(matches!(
		session.preview_profile_edit(
			Revision::INITIAL,
			&registry,
			edit("score", ProfileInput::Text("1,,0"))
		),
		Err(EditorError::InvalidValue {
			source: LexemeError::InvalidFixed,
			..
		})
	));
	assert!(matches!(
		session.apply_profile_edit(Revision::INITIAL, &registry, edit("score", ProfileInput::Text("0.001"))),
		Err(EditorError::Profile(ProfileSessionError::Edit(ProfileEditError::InvalidValue(field)))) if field == "score"
	));
	assert_eq!(session.document().source(), BANK);
	assert_eq!(session.revision(), Revision::INITIAL);
	assert!(!session.can_undo());
}

#[test]
fn preview_and_apply_both_reject_the_full_output_limits() {
	let registry = registry();
	let limits = Limits {
		scalar_bytes: 64,
		..Limits::default()
	};
	let mut session = Session::open(BANK, limits, Some(context())).unwrap();
	let escaped = "&&&&&&&&&&&&&&&&&&&&";
	let request = edit("note", ProfileInput::Text(escaped));
	assert!(matches!(
		session.preview_profile_edit(Revision::INITIAL, &registry, request),
		Err(EditorError::Serialize(_))
	));
	assert!(matches!(
		session.apply_profile_edit(Revision::INITIAL, &registry, request),
		Err(EditorError::Serialize(_))
	));
	assert_eq!(session.document().source(), BANK);
	assert_eq!(session.revision(), Revision::INITIAL);
}

#[test]
fn incomparable_profiles_do_not_select_a_writer() {
	let exact = Profile::from_json(DEFINITION).unwrap();
	let other = Profile::from_json(&DEFINITION.replace(PROFILE_ID, "synthetic-other")).unwrap();
	let ambiguous = ProfileRegistry::new(vec![exact, other]).unwrap();
	let mut session = Session::open(BANK, Limits::default(), Some(context())).unwrap();
	let score = edit("score", ProfileInput::Text("2.00"));
	assert!(matches!(
		session.apply_profile_edit(Revision::INITIAL, &ambiguous, score),
		Err(EditorError::Profile(ProfileSessionError::Ambiguous))
	));
	assert_eq!(session.document().source(), BANK);
}
