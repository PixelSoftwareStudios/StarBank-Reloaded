use std::{
	collections::{BTreeMap, BTreeSet, VecDeque},
	error::Error,
	fmt,
	ops::Range,
};

use starbank_core::is_sc2_account_handle;

use crate::{
	CheckboxStorage, Control, NumberStorage, decimal,
	definition::{Definition, Field, FieldPath, Identity, RuleDefinition},
};

pub(crate) const VERSION: u32 = 1;
pub(crate) const MAX_DEFINITION_BYTES: usize = 64 * 1024;
pub(crate) const MAX_FIELDS: usize = 128;
pub(crate) const MAX_GROUPS: usize = 32;
const MAX_RULES: usize = MAX_FIELDS;
const MAX_PREDICATES: usize = 16;
const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_DESCRIPTION_BYTES: usize = 2048;
const MAX_EXPECTED_BYTES: usize = 2048;
const MAP_FINGERPRINT_HEX_CHARS: usize = 64;

/// A profile definition checked for bounded fields, exact mappings and an acyclic rule graph.
#[derive(Debug)]
pub struct Profile {
	pub(crate) id: String,
	pub(crate) identity: Identity,
	pub(crate) groups: Vec<Group>,
	pub(crate) fields: Vec<Field>,
	pub(crate) ordered_rules: Vec<Rule>,
	pub(crate) derived: BTreeSet<usize>,
	pub(crate) legacy: bool,
}

#[derive(Debug)]
pub(crate) struct Group {
	pub label: String,
	pub description: Option<String>,
	pub fields: Range<usize>,
}

#[derive(Debug)]
pub(crate) struct Rule {
	pub source: usize,
	pub target: usize,
	pub multiplier: i64,
	pub addend: i128,
	pub scale: u32,
}

/// Why a profile definition or registry cannot be used.
#[derive(Debug)]
#[non_exhaustive]
pub enum ProfileError {
	/// The definition input exceeded the limit.
	InputTooLarge,
	/// The profile version is unsupported.
	UnsupportedVersion(u32),
	/// The definition is not valid JSON for this schema.
	Json(serde_json::Error),
	/// The definition is not compatible legacy YAML.
	Yaml(Box<serde_saphyr::Error>),
	/// A declared profile property is missing, too large or invalid.
	Invalid(&'static str),
	/// A profile identifier appears more than once.
	DuplicateProfile(String),
	/// A field identifier appears more than once.
	DuplicateField(String),
	/// Several fields declare the same bank address.
	DuplicatePath,
	/// A rule refers to an unknown field.
	UnknownField(String),
	/// A rule refers to a numeric field without a compatible exact scale.
	IncompatibleRule,
	/// More than one rule writes a dependent field.
	DuplicateRuleTarget(String),
	/// Rules form a dependency cycle.
	RuleCycle,
}

impl fmt::Display for ProfileError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InputTooLarge => formatter.write_str("profile input exceeds the size limit"),
			Self::UnsupportedVersion(version) => {
				write!(formatter, "unsupported profile version {version}")
			}
			Self::Json(source) => write!(formatter, "invalid profile JSON: {source}"),
			Self::Yaml(source) => write!(formatter, "invalid legacy profile YAML: {source}"),
			Self::Invalid(field) => write!(formatter, "invalid profile {field}"),
			Self::DuplicateProfile(id) => write!(formatter, "duplicate profile ID {id}"),
			Self::DuplicateField(id) => write!(formatter, "duplicate profile field ID {id}"),
			Self::DuplicatePath => formatter.write_str("duplicate profile field path"),
			Self::UnknownField(id) => write!(formatter, "rule references unknown field {id}"),
			Self::IncompatibleRule => {
				formatter.write_str("rule requires matching declared numeric scales")
			}
			Self::DuplicateRuleTarget(id) => write!(formatter, "multiple rules write field {id}"),
			Self::RuleCycle => formatter.write_str("profile rules form a dependency cycle"),
		}
	}
}

impl Error for ProfileError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Json(source) => Some(source),
			Self::Yaml(source) => Some(source),
			_ => None,
		}
	}
}

impl Profile {
	/// Loads and validates a version 1 declarative JSON definition.
	///
	/// # Errors
	///
	/// Returns an error for an unsupported version, identity, path, mapping or rule.
	pub fn from_json(source: &str) -> Result<Self, ProfileError> {
		check_input(source)?;
		let definition = serde_json::from_str(source).map_err(ProfileError::Json)?;
		Self::compile(definition, false)
	}

	/// Gets the unique profile identifier declared in the definition.
	#[must_use]
	pub fn id(&self) -> &str {
		&self.id
	}

	pub(crate) fn compile(definition: Definition, legacy: bool) -> Result<Self, ProfileError> {
		let Definition {
			version,
			id,
			identity,
			groups,
			rules,
		} = definition;
		if version != VERSION {
			return Err(ProfileError::UnsupportedVersion(version));
		}
		if !identifier(&id) {
			return Err(ProfileError::Invalid("identifier"));
		}
		validate_identity(&identity)?;
		if groups.is_empty() || groups.len() > MAX_GROUPS || rules.len() > MAX_RULES {
			return Err(ProfileError::Invalid("group or rule count"));
		}

		let mut compiled_groups = Vec::with_capacity(groups.len());
		let mut fields = Vec::new();
		let mut addresses = BTreeSet::new();
		let mut field_ids = BTreeMap::new();
		for group in groups {
			if !label(&group.label)
				|| !description(group.description.as_deref())
				|| group.fields.is_empty()
				|| group.fields.len() > MAX_FIELDS - fields.len()
			{
				return Err(ProfileError::Invalid("group"));
			}
			let start = fields.len();
			for field in group.fields {
				validate_field(&field)?;
				if field_ids.insert(field.id.clone(), fields.len()).is_some() {
					return Err(ProfileError::DuplicateField(field.id));
				}
				if !addresses.insert(field.path.clone()) {
					return Err(ProfileError::DuplicatePath);
				}
				fields.push(field);
			}
			compiled_groups.push(Group {
				label: group.label,
				description: group.description,
				fields: start..fields.len(),
			});
		}

		let (ordered_rules, derived) = compile_rules(rules, &field_ids, &fields)?;
		Ok(Self {
			id,
			identity,
			groups: compiled_groups,
			fields,
			ordered_rules,
			derived,
			legacy,
		})
	}
}

pub(crate) fn check_input(source: &str) -> Result<(), ProfileError> {
	if source.len() > MAX_DEFINITION_BYTES {
		Err(ProfileError::InputTooLarge)
	} else {
		Ok(())
	}
}

fn validate_identity(identity: &Identity) -> Result<(), ProfileError> {
	if !bounded(&identity.author, MAX_IDENTIFIER_BYTES)
		|| !is_sc2_account_handle(&identity.author)
		|| !label(&identity.bank)
		|| identity.bank.contains(['/', '\\', '\0'])
		|| matches!(identity.bank.as_str(), "." | "..")
		|| identity
			.map_version
			.as_deref()
			.is_some_and(|version| !label(version))
		|| identity.fingerprint.as_deref().is_some_and(|fingerprint| {
			fingerprint.len() != MAP_FINGERPRINT_HEX_CHARS
				|| !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
		})
		|| identity.predicates.len() > MAX_PREDICATES
	{
		return Err(ProfileError::Invalid("bank identity"));
	}
	for predicate in &identity.predicates {
		if !valid_path(&predicate.path) || !bounded(&predicate.equals, MAX_EXPECTED_BYTES) {
			return Err(ProfileError::Invalid("match predicate"));
		}
	}
	Ok(())
}

fn validate_field(field: &Field) -> Result<(), ProfileError> {
	if !identifier(&field.id)
		|| !label(&field.label)
		|| !description(field.description.as_deref())
		|| !valid_path(&field.path)
	{
		return Err(ProfileError::Invalid("field"));
	}
	match &field.control {
		Control::Number {
			storage,
			min,
			max,
			scale,
		} => {
			let precision = match storage {
				NumberStorage::Int if scale.is_none_or(|scale| scale == 0) => 0,
				NumberStorage::Int => return Err(ProfileError::Invalid("integer scale")),
				NumberStorage::Fixed | NumberStorage::BankValue => {
					scale.unwrap_or(decimal::MAX_SCALE)
				}
			};
			let min = min.as_deref();
			let max = max.as_deref();
			if precision > decimal::MAX_SCALE
				|| min.is_some_and(|min| !decimal::in_range(min, precision, None, max))
				|| max.is_some_and(|max| !decimal::in_range(max, precision, min, None))
			{
				return Err(ProfileError::Invalid("numeric limits"));
			}
		}
		Control::Checkbox {
			storage,
			checked,
			unchecked,
		} => {
			if checked == unchecked
				|| [checked, unchecked].iter().any(|value| {
					!bounded(value, MAX_EXPECTED_BYTES)
						|| (*storage == CheckboxStorage::Int
							&& !decimal::in_range(value, 0, None, None))
				}) {
				return Err(ProfileError::Invalid("checkbox mapping"));
			}
		}
		Control::Text { .. } => {}
	}
	Ok(())
}

fn valid_path(path: &FieldPath) -> bool {
	[Some(&path.section), Some(&path.key), path.child.as_ref()]
		.into_iter()
		.flatten()
		.all(|part| bounded(&part.name, MAX_LABEL_BYTES))
}

fn identifier(value: &str) -> bool {
	!value.is_empty()
		&& value.len() <= MAX_IDENTIFIER_BYTES
		&& value
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn description(value: Option<&str>) -> bool {
	value.is_none_or(|value| bounded(value, MAX_DESCRIPTION_BYTES))
}

fn label(value: &str) -> bool {
	!value.trim().is_empty() && bounded(value, MAX_LABEL_BYTES)
}

fn bounded(value: &str, max_bytes: usize) -> bool {
	value.len() <= max_bytes && xml_scalars(value)
}

pub(crate) fn xml_scalars(value: &str) -> bool {
	value.chars().all(|scalar| {
		matches!(scalar,
			'\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
		)
	})
}

fn compile_rules(
	rules: Vec<RuleDefinition>,
	ids: &BTreeMap<String, usize>,
	fields: &[Field],
) -> Result<(Vec<Rule>, BTreeSet<usize>), ProfileError> {
	let mut derived = BTreeSet::new();
	let mut edges: Vec<Vec<Rule>> = (0..fields.len()).map(|_| Vec::new()).collect();
	for rule in rules {
		let key = |id: String| ids.get(&id).copied().ok_or(ProfileError::UnknownField(id));
		let source = key(rule.source)?;
		let target = key(rule.target)?;
		if !derived.insert(target) {
			return Err(ProfileError::DuplicateRuleTarget(fields[target].id.clone()));
		}
		let Some(scale) = fields[source].control.rule_scale() else {
			return Err(ProfileError::IncompatibleRule);
		};
		if fields[target].control.rule_scale() != Some(scale) {
			return Err(ProfileError::IncompatibleRule);
		}
		let Some(addend) =
			decimal::Decimal::parse(&rule.addend, scale).and_then(|value| value.scaled(scale))
		else {
			return Err(ProfileError::Invalid("rule addend"));
		};
		edges[source].push(Rule {
			source,
			target,
			multiplier: rule.multiplier,
			addend,
			scale,
		});
	}

	// Each target has one writer. Process incoming roots first and reject any residual cycle.
	let mut queue: VecDeque<_> = (0..fields.len())
		.filter(|index| !derived.contains(index))
		.collect();
	let mut ordered = Vec::with_capacity(derived.len());
	while let Some(index) = queue.pop_front() {
		for edge in std::mem::take(&mut edges[index]) {
			queue.push_back(edge.target);
			ordered.push(edge);
		}
	}
	if ordered.len() != derived.len() {
		return Err(ProfileError::RuleCycle);
	}
	Ok((ordered, derived))
}
