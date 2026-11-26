use sc2bank::{Limits, ValueEdit, parse, serialize};
use serde_json::Value;
use starbank_profiles::{
	BankIdentity, BoundProfile, Confidence, MatchOutcome, Profile, ProfileEditError, ProfileError,
	ProfileInput, ProfileRegistry,
};

const DEFINITION: &str = include_str!("fixtures/training.json");
const TRAINING_BANK: &[u8] = include_bytes!("fixtures/training.sc2bank");
const LEGACY: &str = include_str!("fixtures/legacy.yaml");
const AUTHOR: &str = "9-S2-1-123";
const BANK: &str = "TrainingBank";
const MAP_VERSION: &str = "fixture-map-v2";
const FINGERPRINT: &str = "9ce9b554c0fa4c0b8d8e48dceece6138c15e091a153a32dcb3223c84e7f41db0";

fn identity() -> BankIdentity<'static> {
	BankIdentity {
		author: Some(AUTHOR),
		bank_name: BANK,
		..BankIdentity::default()
	}
}

fn profile() -> Profile {
	Profile::from_json(DEFINITION).unwrap()
}

fn definition() -> Value {
	serde_json::from_str(DEFINITION).unwrap()
}

fn matched(outcome: MatchOutcome<'_>) -> BoundProfile<'_> {
	let MatchOutcome::Matched(profile) = outcome else {
		panic!("expected profile match, got {outcome:?}");
	};
	profile
}

#[test]
fn dependent_updates_keep_exact_decimal_values_in_one_proposal() {
	let bank = parse(TRAINING_BANK, Limits::default()).unwrap();
	let registry = ProfileRegistry::new(vec![profile()]).unwrap();
	let binding = matched(registry.match_bank(&bank, &identity()));
	assert_eq!(binding.id(), "synthetic-training");
	assert_eq!(binding.confidence(), Confidence::IdentityAndSchema);
	let first = binding.groups().next().unwrap();
	assert_eq!(first.label, "Training progress");
	assert!(first.fields[0].read_only);
	assert_eq!(first.fields[1].description, Some("Synthetic score"));
	assert_eq!(first.fields[1].lexeme, "01.00");

	let replacement = "9007199254740993.50";
	let proposal = binding
		.propose_edit("score", ProfileInput::Text(replacement))
		.unwrap();
	assert_eq!(
		proposal
			.iter()
			.map(|edit| (
				edit.field_id.as_str(),
				edit.previous.as_str(),
				edit.value.as_str()
			))
			.collect::<Vec<_>>(),
		[
			("reward", "6.25", "54043195528445961.25"),
			("score", "01.00", replacement),
			("total", "2.25", "18014398509481987.25"),
		]
	);
	assert_eq!(proposal[1].target, first.fields[1].target);
	assert_eq!(bank.source(), TRAINING_BANK);

	let edits = proposal
		.iter()
		.map(|edit| ValueEdit {
			id: edit.target,
			value: &edit.value,
		})
		.collect::<Vec<_>>();
	let written = serialize(&bank, &edits, Limits::default()).unwrap();
	let untouched = core::str::from_utf8(TRAINING_BANK).unwrap();
	let expected = untouched
		.replace("01.00", replacement)
		.replace("6.25", &proposal[0].value)
		.replace("2.25", &proposal[2].value);
	assert_eq!(written, expected.as_bytes());
	let reopened = parse(&written, Limits::default()).unwrap();
	let rebound = matched(registry.match_bank(&reopened, &identity()));
	assert_eq!(
		rebound.groups().next().unwrap().fields[1].lexeme,
		replacement
	);
}

#[test]
fn dependent_bounds_fail_before_any_proposal_escapes() {
	let mut schema = definition();
	schema["groups"][0]["fields"][0]["control"]["max"] = "7.00".into();
	let registry =
		ProfileRegistry::new(vec![Profile::from_json(&schema.to_string()).unwrap()]).unwrap();
	let bank = parse(TRAINING_BANK, Limits::default()).unwrap();
	let binding = matched(registry.match_bank(&bank, &identity()));
	assert_eq!(
		binding.propose_edit("score", ProfileInput::Text("10.00")),
		Err(ProfileEditError::InvalidValue("reward".into()))
	);
	assert_eq!(
		binding.propose_edit("score", ProfileInput::Text("0.001")),
		Err(ProfileEditError::InvalidValue("score".into()))
	);
	assert_eq!(
		binding.propose_edit("score", ProfileInput::Text("1,000.00")),
		Err(ProfileEditError::InvalidValue("score".into()))
	);
	assert_eq!(
		binding.propose_edit("score", ProfileInput::Text("-9007199254740994.01")),
		Err(ProfileEditError::InvalidValue("score".into()))
	);
	assert_eq!(
		binding.propose_edit("reward", ProfileInput::Text("7.00")),
		Err(ProfileEditError::ReadOnly)
	);
	assert_eq!(
		binding.propose_edit("other", ProfileInput::Text("7.00")),
		Err(ProfileEditError::UnknownField)
	);
	assert_eq!(bank.source(), TRAINING_BANK);
}

#[test]
fn exact_arithmetic_denies_overflow_and_keeps_negative_precision() {
	let mut schema = definition();
	schema["rules"][0]["multiplier"] = i64::MAX.into();
	schema["rules"][1]["multiplier"] = i64::MAX.into();
	let registry =
		ProfileRegistry::new(vec![Profile::from_json(&schema.to_string()).unwrap()]).unwrap();
	let bank = parse(TRAINING_BANK, Limits::default()).unwrap();
	let binding = matched(registry.match_bank(&bank, &identity()));
	assert_eq!(
		binding.propose_edit("score", ProfileInput::Text("9007199254740994.00")),
		Err(ProfileEditError::Overflow("reward".into()))
	);

	let normal = ProfileRegistry::new(vec![profile()]).unwrap();
	let binding = matched(normal.match_bank(&bank, &identity()));
	let proposals = binding
		.propose_edit("score", ProfileInput::Text("-01.00"))
		.unwrap();
	assert_eq!(proposals[0].value, "-5.75");
	assert_eq!(proposals[1].value, "-01.00");
	assert_eq!(proposals[2].value, "-1.75");
}

#[test]
fn controls_preserve_declared_checkbox_and_text_storage() {
	let bank = parse(TRAINING_BANK, Limits::default()).unwrap();
	let registry = ProfileRegistry::new(vec![profile()]).unwrap();
	let binding = matched(registry.match_bank(&bank, &identity()));
	let options = binding.groups().last().unwrap();
	assert_eq!(options.description, Some("A fixture-only group"));
	assert_eq!(options.fields[0].checked(), Some(true));
	assert_eq!(options.fields[1].lexeme, "A, B & C");
	let checkbox = binding
		.propose_edit("enabled", ProfileInput::Checked(false))
		.unwrap();
	assert_eq!(checkbox[0].value, "no");
	assert!(
		binding
			.propose_edit("enabled", ProfileInput::Checked(true))
			.unwrap()
			.is_empty()
	);
	assert_eq!(
		binding.propose_edit("enabled", ProfileInput::Text("no")),
		Err(ProfileEditError::WrongInput)
	);
	let target_text = " x, y\t&\nz  ";
	let text = binding
		.propose_edit("note", ProfileInput::Text(target_text))
		.unwrap();
	assert_eq!(text[0].value, target_text);
	assert_eq!(
		binding.propose_edit("note", ProfileInput::Text("\0")),
		Err(ProfileEditError::InvalidValue("note".into()))
	);
}

#[test]
fn registry_requires_identity_predicates_and_deterministically_falls_back() {
	let bank = parse(TRAINING_BANK, Limits::default()).unwrap();
	let registry = ProfileRegistry::new(vec![profile()]).unwrap();
	for weak in [
		BankIdentity {
			author: None,
			..identity()
		},
		BankIdentity {
			author: Some("9-S2-1-456"),
			..identity()
		},
		BankIdentity {
			bank_name: "trainingbank",
			..identity()
		},
	] {
		assert!(matches!(
			registry.match_bank(&bank, &weak),
			MatchOutcome::Unknown
		));
	}
	let different = core::str::from_utf8(TRAINING_BANK)
		.unwrap()
		.replace("fixture-v1", "fixture-v2");
	let different = parse(different.as_bytes(), Limits::default()).unwrap();
	assert!(matches!(
		registry.match_bank(&different, &identity()),
		MatchOutcome::Incompatible
	));

	let mut qualified = definition();
	qualified["id"] = "synthetic-versioned".into();
	qualified["identity"]["map_version"] = MAP_VERSION.into();
	qualified["identity"]["fingerprint"] = FINGERPRINT.into();
	let general_and_qualified = ProfileRegistry::new(vec![
		Profile::from_json(&qualified.to_string()).unwrap(),
		profile(),
	])
	.unwrap();
	assert_eq!(
		matched(general_and_qualified.match_bank(&bank, &identity())).id(),
		"synthetic-training"
	);
	let evidence = BankIdentity {
		map_version: Some(MAP_VERSION),
		fingerprint: Some(FINGERPRINT),
		..identity()
	};
	assert_eq!(
		matched(general_and_qualified.match_bank(&bank, &evidence)).confidence(),
		Confidence::VersionAndFingerprint
	);

	let mut peer = qualified.clone();
	peer["id"] = "another-versioned".into();
	let choices = ProfileRegistry::new(vec![
		Profile::from_json(&qualified.to_string()).unwrap(),
		profile(),
		Profile::from_json(&peer.to_string()).unwrap(),
	])
	.unwrap();
	assert!(
		matches!(choices.match_bank(&bank, &evidence), MatchOutcome::Ambiguous(ids) if ids == ["another-versioned", "synthetic-versioned"])
	);
}

#[test]
fn occurrences_avoid_duplicate_name_coercion_and_actual_aliases() {
	let text = core::str::from_utf8(TRAINING_BANK).unwrap().replace(
		"<Score fixed='01.00' />",
		"<Score fixed='01.00' /><Score fixed='02.00' />",
	);
	let bank = parse(text.as_bytes(), Limits::default()).unwrap();
	let registry = ProfileRegistry::new(vec![profile()]).unwrap();
	assert!(matches!(
		registry.match_bank(&bank, &identity()),
		MatchOutcome::Incompatible
	));

	let mut schema = definition();
	schema["groups"][0]["fields"][1]["path"]["child"]["occurrence"] = 1.into();
	let registry =
		ProfileRegistry::new(vec![Profile::from_json(&schema.to_string()).unwrap()]).unwrap();
	let binding = matched(registry.match_bank(&bank, &identity()));
	assert_eq!(binding.groups().next().unwrap().fields[1].lexeme, "02.00");

	let mut alias = schema["groups"][0]["fields"][1].clone();
	alias["id"] = "alias".into();
	alias["path"]["key"]["occurrence"] = 0.into();
	schema["groups"][1]["fields"]
		.as_array_mut()
		.unwrap()
		.push(alias);
	let registry =
		ProfileRegistry::new(vec![Profile::from_json(&schema.to_string()).unwrap()]).unwrap();
	assert!(matches!(
		registry.match_bank(&bank, &identity()),
		MatchOutcome::Incompatible
	));
}

#[test]
fn invalid_schema_and_dependency_definitions_are_rejected() {
	let mut schema = definition();
	schema["version"] = 2.into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::UnsupportedVersion(2))
	));
	schema["version"] = 1.into();
	schema["script"] = "unrecognized-property".into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::Json(_))
	));
	assert!(matches!(
		Profile::from_json(&" ".repeat(64 * 1024 + 1)),
		Err(ProfileError::InputTooLarge)
	));

	let mut schema = definition();
	schema["rules"][0]["source"] = "no-field".into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::UnknownField(_))
	));
	schema["rules"][0]["source"] = "total".into();
	schema["rules"][1]["source"] = "reward".into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::RuleCycle)
	));
	schema["rules"][1]["target"] = "reward".into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::DuplicateRuleTarget(_))
	));

	let mut schema = definition();
	schema["groups"][0]["fields"][0]["control"]["scale"] = 3.into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::IncompatibleRule)
	));
	schema["groups"][0]["fields"][0]["control"]["scale"] = 2.into();
	schema["groups"][0]["fields"][1]["control"]["min"] = "999999999999999999999999.00".into();
	assert!(matches!(
		Profile::from_json(&schema.to_string()),
		Err(ProfileError::Invalid("numeric limits"))
	));
	assert!(matches!(
		ProfileRegistry::new(vec![profile(), profile()]),
		Err(ProfileError::DuplicateProfile(_))
	));
}

#[test]
fn legacy_fields_migrate_without_relaxing_identity_or_storage_validation() {
	let bank = parse(br#"<Bank><Section name="Counts"><Key name="coins"><Value int="0001"/></Key><Key name="hints"><Value int="0"/></Key><Key name="display"><Value string="Synthetic"/></Key></Section></Bank>"#, Limits::default()).unwrap();
	let legacy = Profile::import_legacy_yaml(LEGACY, "fixture-legacy", AUTHOR).unwrap();
	let registry = ProfileRegistry::new(vec![legacy]).unwrap();
	let identity = BankIdentity {
		author: Some(AUTHOR),
		bank_name: "LEGACY-FIXTURE",
		..BankIdentity::default()
	};
	let binding = matched(registry.match_bank(&bank, &identity));
	assert_eq!(binding.confidence(), Confidence::Legacy);
	assert_eq!(binding.groups().next().unwrap().fields[0].lexeme, "0001");
	assert_eq!(
		binding
			.propose_edit("legacy-0-1", ProfileInput::Checked(true))
			.unwrap()[0]
			.value,
		"1"
	);
	assert!(matches!(
		registry.match_bank(
			&bank,
			&BankIdentity {
				author: None,
				..identity
			}
		),
		MatchOutcome::Unknown
	));
	assert!(matches!(
		Profile::import_legacy_yaml(LEGACY, "fixture-legacy", ""),
		Err(ProfileError::Invalid("bank identity"))
	));
	for malformed in [
		LEGACY.replace(
			"map: \"legacy-fixture\"",
			"map: &alias-name \"legacy-fixture\"",
		),
		LEGACY.replace(
			"map: \"legacy-fixture\"",
			"map: !include \"legacy-fixture\"",
		),
		format!("map: duplicate\n{LEGACY}"),
		format!("{LEGACY}\n---\n{LEGACY}"),
	] {
		assert!(
			matches!(
				Profile::import_legacy_yaml(&malformed, "fixture-legacy", AUTHOR),
				Err(ProfileError::Yaml(_))
			),
			"accepted {malformed}"
		);
	}
}

#[test]
fn legacy_stem_matching_uses_simple_unicode_lowercase() {
	let bank = parse(
		br#"<Bank><Section name="Counts"><Key name="coins"><Value int="1"/></Key><Key name="hints"><Value int="0"/></Key><Key name="display"><Value string="Synthetic"/></Key></Section></Bank>"#,
		Limits::default(),
	).unwrap();
	let legacy = LEGACY.replace("legacy-fixture", "ΟΣİ");
	let registry = ProfileRegistry::new(vec![
		Profile::import_legacy_yaml(&legacy, "fixture-legacy-unicode", AUTHOR).unwrap(),
	])
	.unwrap();
	let identity = BankIdentity {
		author: Some(AUTHOR),
		bank_name: "οσi",
		..BankIdentity::default()
	};
	assert!(matches!(
		registry.match_bank(&bank, &identity),
		MatchOutcome::Matched(_)
	));
}
