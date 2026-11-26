use std::{error::Error, fmt};

use sc2bank::ValueId;

use crate::{BoundProfile, Control, decimal, profile::xml_scalars};

const MAX_FIELD_VALUE_BYTES: usize = 1024 * 1024;

/// An explicit edit to one friendly control.
#[derive(Clone, Copy, Debug)]
pub enum ProfileInput<'a> {
	/// Text or a canonical exact numeric lexeme.
	Text(&'a str),
	/// Checkbox selection.
	Checked(bool),
}

/// One member of an all-or-none profile edit proposal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedValueEdit {
	/// Friendly field identifier within the profile.
	pub field_id: String,
	/// Exact value occurrence in the bound document.
	pub target: ValueId,
	/// Previous XML-decoded lexeme, for preview and stale checks.
	pub previous: String,
	/// Replacement XML-decoded lexeme.
	pub value: String,
}

/// Why a complete edit proposal could not be formed.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProfileEditError {
	/// The profile has no field with this identifier.
	UnknownField,
	/// The field is declared read-only or is computed by a dependency rule.
	ReadOnly,
	/// The input kind differs from the field's control kind.
	WrongInput,
	/// The requested or derived value falls outside its declared mapping.
	InvalidValue(String),
	/// Exact dependency arithmetic exceeded its checked representation.
	Overflow(String),
}

impl fmt::Display for ProfileEditError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnknownField => formatter.write_str("unknown profile field"),
			Self::ReadOnly => formatter.write_str("profile field is read-only"),
			Self::WrongInput => formatter.write_str("input does not match the profile control"),
			Self::InvalidValue(field) => {
				write!(formatter, "invalid value for profile field {field}")
			}
			Self::Overflow(field) => {
				write!(formatter, "dependency overflow for profile field {field}")
			}
		}
	}
}

impl Error for ProfileEditError {}

impl BoundProfile<'_> {
	/// Previews a complete edit and all affected derived fields without modifying the bank.
	///
	/// The caller applies the proposal as one revision-checked editor transaction. Numeric
	/// text uses canonical, ungrouped lexemes. No-op changes are omitted.
	///
	/// # Errors
	///
	/// Rejects inaccessible controls, invalid values, and arithmetic overflow.
	pub fn propose_edit(
		&self,
		field_id: &str,
		input: ProfileInput<'_>,
	) -> Result<Vec<ProposedValueEdit>, ProfileEditError> {
		let start = self
			.fields
			.iter()
			.position(|field| field.id == field_id)
			.ok_or(ProfileEditError::UnknownField)?;
		let field = &self.fields[start];
		if field.read_only {
			return Err(ProfileEditError::ReadOnly);
		}

		let replacement = match (field.control, input) {
			(
				Control::Checkbox {
					checked, unchecked, ..
				},
				ProfileInput::Checked(value),
			) => {
				if value {
					checked
				} else {
					unchecked
				}
			}
			(Control::Number { .. } | Control::Text { .. }, ProfileInput::Text(value)) => value,
			_ => return Err(ProfileEditError::WrongInput),
		};
		if replacement.len() > MAX_FIELD_VALUE_BYTES
			|| !field.control.accepts(field.kind, replacement)
			|| !xml_scalars(replacement)
		{
			return Err(ProfileEditError::InvalidValue(field.id.to_owned()));
		}

		let mut replacements = vec![None; self.fields.len()];
		replacements[start] = Some(replacement.to_owned());
		for rule in &self.profile.ordered_rules {
			let Some(source) = replacements[rule.source].as_deref() else {
				continue;
			};
			let target = &self.fields[rule.target];
			let outcome = decimal::Decimal::parse(source, rule.scale)
				.and_then(|source| source.scaled(rule.scale))
				.and_then(|source| source.checked_mul(i128::from(rule.multiplier)))
				.and_then(|product| product.checked_add(rule.addend))
				.ok_or_else(|| ProfileEditError::Overflow(target.id.to_owned()))?;
			let precision = usize::try_from(rule.scale)
				.map_err(|_| ProfileEditError::Overflow(target.id.to_owned()))?;
			let derived = decimal::render(outcome, precision);
			if !target.control.accepts(target.kind, &derived) {
				return Err(ProfileEditError::InvalidValue(target.id.to_owned()));
			}
			replacements[rule.target] = Some(derived);
		}

		Ok(self
			.fields
			.iter()
			.zip(replacements)
			.filter_map(|(field, value)| {
				value
					.filter(|value| value != field.lexeme)
					.map(|value| ProposedValueEdit {
						field_id: field.id.to_owned(),
						target: field.target,
						previous: field.lexeme.to_owned(),
						value,
					})
			})
			.collect())
	}
}
