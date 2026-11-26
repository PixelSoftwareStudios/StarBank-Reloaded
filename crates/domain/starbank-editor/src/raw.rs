use std::collections::BTreeMap;

use sc2bank::{BankDocument, ValueKind};

use crate::{EditorError, normalize_value};

/// Numeric type whose exact XML attribute failed Raw View validation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum RawNumberKind {
	/// An integer attribute.
	Integer,
	/// A fixed-point attribute.
	Fixed,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct InvalidNumber<'bank> {
	section: Option<&'bank str>,
	key: Option<&'bank str>,
	node: &'bank str,
	kind: RawNumberKind,
	lexeme: &'bank str,
}

pub(crate) fn validate_raw_numbers(
	previous: &BankDocument,
	candidate: &BankDocument,
) -> Result<(), EditorError> {
	let mut additions = invalid_numbers(candidate).peekable();
	if additions.peek().is_none() {
		return Ok(());
	}

	// Raw replacement has no stable IDs. Match exact inherited lexemes by projected
	// path and multiplicity, so an unrelated change can preserve old malformed data,
	// and duplicating such a value does not grant it another match.
	let mut existing = BTreeMap::new();
	for (number, _) in invalid_numbers(previous) {
		*existing.entry(number).or_insert(0usize) += 1;
	}
	for (number, byte) in additions {
		let count = existing
			.get_mut(&number)
			.filter(|count| **count > 0)
			.ok_or(EditorError::InvalidRawNumber {
				kind: number.kind,
				byte,
			})?;
		*count -= 1;
	}
	Ok(())
}

fn invalid_numbers(bank: &BankDocument) -> impl Iterator<Item = (InvalidNumber<'_>, usize)> {
	bank.sections().iter().flat_map(|section| {
		section.keys().iter().flat_map(move |key| {
			key.values().iter().filter_map(move |node| {
				let attribute = node.attribute()?;
				let kind = match attribute.kind() {
					ValueKind::Int => RawNumberKind::Integer,
					ValueKind::Fixed => RawNumberKind::Fixed,
					_ => return None,
				};
				let lexeme = attribute.value();
				// Structured input can normalize grouping. Raw authors supply the final
				// attribute lexeme, and the accepted XML is preserved byte-for-byte.
				if normalize_value(attribute.kind(), lexeme).is_ok_and(|exact| exact == lexeme) {
					return None;
				}
				Some((
					InvalidNumber {
						section: section.name(),
						key: key.name(),
						node: node.name(),
						kind,
						lexeme,
					},
					attribute.source_range().start,
				))
			})
		})
	})
}
