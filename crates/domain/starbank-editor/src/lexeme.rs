use std::{borrow::Cow, error::Error, fmt};

use sc2bank::ValueKind;

const GROUP_SEPARATOR: char = ',';
const DECIMAL_SEPARATOR: char = '.';
const NEGATIVE_SIGN: char = '-';
const DIGITS_PER_GROUP: usize = 3;

/// An invalid or unsupported value for a structured edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LexemeError {
	/// Expected an integer with optional minus sign and valid comma grouping.
	InvalidInteger,
	/// Expected an exact decimal with optional minus sign and valid comma grouping.
	InvalidFixed,
	/// The value's type has no supported structured edit syntax.
	UnsupportedKind,
}

impl fmt::Display for LexemeError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::InvalidInteger => "expected an integer with optional comma grouping",
			Self::InvalidFixed => "expected a decimal with optional comma grouping",
			Self::UnsupportedKind => "this value type has no supported structured edit syntax",
		})
	}
}

impl Error for LexemeError {}

/// Reports whether structured input has a supported lexical grammar for the type.
#[must_use]
pub fn is_value_editable(kind: &ValueKind) -> bool {
	matches!(
		kind,
		ValueKind::Int | ValueKind::Fixed | ValueKind::String | ValueKind::Text
	)
}

/// Validates an intentional structured edit and removes numeric grouping without rounding.
///
/// ASCII integers and fixed-point values accept a leading minus sign and comma groups in
/// the integer part. An optional fractional part for fixed-point values uses a decimal
/// dot followed by one or more digits. String and text values retain their input verbatim.
/// XML validity and document limits are checked when the edited value is serialized.
///
/// # Errors
///
/// Returns an error for malformed numbers or value types without a supported edit syntax.
pub fn normalize_value<'value>(
	kind: &ValueKind,
	value: &'value str,
) -> Result<Cow<'value, str>, LexemeError> {
	let magnitude = value.strip_prefix(NEGATIVE_SIGN).unwrap_or(value);
	match kind {
		ValueKind::Int if integer_digits(magnitude) => Ok(without_grouping(value)),
		ValueKind::Int => Err(LexemeError::InvalidInteger),
		ValueKind::Fixed if fixed_digits(magnitude) => Ok(without_grouping(value)),
		ValueKind::Fixed => Err(LexemeError::InvalidFixed),
		ValueKind::String | ValueKind::Text => Ok(Cow::Borrowed(value)),
		_ => Err(LexemeError::UnsupportedKind),
	}
}

fn fixed_digits(value: &str) -> bool {
	match value.split_once(DECIMAL_SEPARATOR) {
		Some((integer, fraction)) => integer_digits(integer) && ascii_digits(fraction),
		None => integer_digits(value),
	}
}

fn integer_digits(value: &str) -> bool {
	match value.split_once(GROUP_SEPARATOR) {
		Some((leading, trailing)) => {
			leading.len() <= DIGITS_PER_GROUP
				&& ascii_digits(leading)
				&& trailing
					.split(GROUP_SEPARATOR)
					.all(|group| group.len() == DIGITS_PER_GROUP && ascii_digits(group))
		}
		None => ascii_digits(value),
	}
}

fn ascii_digits(value: &str) -> bool {
	!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn without_grouping(value: &str) -> Cow<'_, str> {
	if value.contains(GROUP_SEPARATOR) {
		Cow::Owned(value.replace(GROUP_SEPARATOR, ""))
	} else {
		Cow::Borrowed(value)
	}
}
