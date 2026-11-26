use serde::Deserialize;
use serde_saphyr::DuplicateKeyPolicy;

use crate::{
	CheckboxStorage, Control, NumberStorage, Profile, ProfileError, TextStorage,
	definition::{Definition, Field, FieldPath, GroupDefinition, Identity, Occurrence},
	profile::{MAX_DEFINITION_BYTES, MAX_FIELDS, MAX_GROUPS, VERSION, check_input},
};

const LEGACY_MAX_DEPTH: usize = 12;
const LEGACY_MAX_EVENTS: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Legacy {
	map: String,
	groups: Vec<LegacyGroup>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyGroup {
	label: String,
	fields: Vec<LegacyField>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyField {
	label: String,
	section: String,
	key: String,
	#[serde(rename = "type")]
	kind: LegacyKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyKind {
	Number,
	Checkbox,
	String,
}

impl Profile {
	/// Imports an archived Go-compatible YAML definition and binds it to an explicit author.
	///
	/// The caller supplies a profile ID and trusted author because the old shape only
	/// carried a bank filename. Fields require an unambiguous section, key and child.
	///
	/// # Errors
	///
	/// Rejects malformed, excessively large or ambiguous definitions, unsupported
	/// field types, and invalid author handles.
	pub fn import_legacy_yaml(source: &str, id: &str, author: &str) -> Result<Self, ProfileError> {
		check_input(source)?;
		let options = serde_saphyr::options! {
			budget: serde_saphyr::budget! {
				max_events: LEGACY_MAX_EVENTS,
				max_nodes: LEGACY_MAX_EVENTS,
				max_total_scalar_bytes: MAX_DEFINITION_BYTES,
				max_depth: LEGACY_MAX_DEPTH,
				max_aliases: 0,
				max_anchors: 0,
				max_merge_keys: 0,
				max_documents: 1,
			},
			duplicate_keys: DuplicateKeyPolicy::Error,
			reject_unsupported_tags: true,
			reject_tagged_field_names: true,
		};
		let schema: Legacy = serde_saphyr::from_str_with_options(source, options)
			.map_err(|error| ProfileError::Yaml(Box::new(error)))?;
		if schema.groups.len() > MAX_GROUPS
			|| schema
				.groups
				.iter()
				.map(|group| group.fields.len())
				.sum::<usize>()
				> MAX_FIELDS
		{
			return Err(ProfileError::Invalid("group or field count"));
		}
		let groups = schema
			.groups
			.into_iter()
			.enumerate()
			.map(|(group_idx, group)| GroupDefinition {
				label: group.label,
				description: None,
				fields: group
					.fields
					.into_iter()
					.enumerate()
					.map(|(field_idx, field)| Field {
						id: format!("legacy-{group_idx}-{field_idx}"),
						label: field.label,
						description: None,
						path: FieldPath {
							section: Occurrence {
								name: field.section,
								occurrence: None,
							},
							key: Occurrence {
								name: field.key,
								occurrence: None,
							},
							child: None,
						},
						control: field.kind.control(),
						read_only: false,
					})
					.collect(),
			})
			.collect();

		Self::compile(
			Definition {
				version: VERSION,
				id: id.to_owned(),
				identity: Identity {
					author: author.to_owned(),
					bank: schema.map,
					map_version: None,
					fingerprint: None,
					predicates: Vec::new(),
				},
				groups,
				rules: Vec::new(),
			},
			true,
		)
	}
}

impl LegacyKind {
	fn control(self) -> Control {
		match self {
			Self::Number => Control::Number {
				storage: NumberStorage::BankValue,
				min: None,
				max: None,
				scale: None,
			},
			Self::Checkbox => Control::Checkbox {
				storage: CheckboxStorage::BankValue,
				checked: "1".into(),
				unchecked: "0".into(),
			},
			Self::String => Control::Text {
				storage: TextStorage::BankValue,
			},
		}
	}
}
