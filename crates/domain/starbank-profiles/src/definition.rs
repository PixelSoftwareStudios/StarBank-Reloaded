use sc2bank::ValueKind;
use serde::Deserialize;

use crate::decimal;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Definition {
	pub version: u32,
	pub id: String,
	pub identity: Identity,
	pub groups: Vec<GroupDefinition>,
	#[serde(default)]
	pub rules: Vec<RuleDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
	pub author: String,
	pub bank: String,
	pub map_version: Option<String>,
	pub fingerprint: Option<String>,
	#[serde(default)]
	pub predicates: Vec<Predicate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Predicate {
	pub path: FieldPath,
	pub equals: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GroupDefinition {
	pub label: String,
	pub description: Option<String>,
	pub fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Field {
	pub id: String,
	pub label: String,
	pub description: Option<String>,
	pub path: FieldPath,
	pub control: Control,
	#[serde(default)]
	pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldPath {
	pub section: Occurrence,
	pub key: Occurrence,
	pub child: Option<Occurrence>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Occurrence {
	pub name: String,
	pub occurrence: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleDefinition {
	pub source: String,
	pub target: String,
	pub multiplier: i64,
	pub addend: String,
}

/// The presentation and exact storage mapping of a profile field.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum Control {
	/// An exact integer or fixed-point number.
	Number {
		/// SC2 attribute kind accepted by this field.
		storage: NumberStorage,
		/// Exact lower bound, if configured.
		min: Option<String>,
		/// Exact upper bound, if configured.
		max: Option<String>,
		/// Configured maximum fractional precision, if any.
		scale: Option<u32>,
	},
	/// A checkbox with two declared, distinct storage lexemes.
	Checkbox {
		/// SC2 attribute kind accepted by this field.
		storage: CheckboxStorage,
		/// Exact lexeme for a selected checkbox.
		checked: String,
		/// Exact lexeme for a cleared checkbox.
		unchecked: String,
	},
	/// A string preserved without normalization.
	Text {
		/// SC2 attribute kind accepted by this field.
		storage: TextStorage,
	},
}

/// Numeric storage kind required by a profile field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberStorage {
	/// An SC2 integer.
	Int,
	/// An SC2 fixed-point value.
	Fixed,
	/// Accepts either numeric kind from a legacy definition.
	BankValue,
}

/// Checkbox storage kind required by a profile field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckboxStorage {
	/// An SC2 flag.
	Flag,
	/// An SC2 integer.
	Int,
	/// An SC2 string.
	String,
	/// Accepts a flag, integer, or string from a legacy definition.
	BankValue,
}

/// Text storage kind required by a profile field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextStorage {
	/// An SC2 string.
	String,
	/// SC2 text.
	Text,
	/// Accepts text or a string from a legacy definition.
	BankValue,
}

impl Control {
	pub(crate) fn accepts(&self, kind: &ValueKind, value: &str) -> bool {
		match self {
			Self::Number {
				storage,
				min,
				max,
				scale,
			} => {
				let precision = match (storage, kind) {
					(NumberStorage::Int | NumberStorage::BankValue, ValueKind::Int) => 0,
					(NumberStorage::Fixed | NumberStorage::BankValue, ValueKind::Fixed) => {
						scale.unwrap_or(decimal::MAX_SCALE)
					}
					_ => return false,
				};
				decimal::in_range(value, precision, min.as_deref(), max.as_deref())
			}
			Self::Checkbox {
				storage,
				checked,
				unchecked,
			} => {
				if value != checked && value != unchecked {
					return false;
				}
				match (storage, kind) {
					(CheckboxStorage::Int | CheckboxStorage::BankValue, ValueKind::Int) => {
						[checked, unchecked]
							.iter()
							.all(|state| decimal::in_range(state, 0, None, None))
					}
					(CheckboxStorage::Flag | CheckboxStorage::BankValue, ValueKind::Flag)
					| (CheckboxStorage::String | CheckboxStorage::BankValue, ValueKind::String) => true,
					_ => false,
				}
			}
			Self::Text { storage } => matches!(
				(storage, kind),
				(
					TextStorage::String | TextStorage::BankValue,
					ValueKind::String
				) | (TextStorage::Text | TextStorage::BankValue, ValueKind::Text)
			),
		}
	}

	pub(crate) fn rule_scale(&self) -> Option<u32> {
		match self {
			Self::Number {
				storage: NumberStorage::Int,
				..
			} => Some(0),
			Self::Number {
				storage: NumberStorage::Fixed,
				scale,
				..
			} => *scale,
			_ => None,
		}
	}
}
