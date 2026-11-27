use std::{collections::BTreeMap, ops::Range};

use crate::{
	BankDocument, LimitKind, Limits, ParseErrorKind, SerializeError, ValueId, ValueKind, ValueNode,
	parse, parse::is_xml_character,
};

/// A replacement for one value attribute in the document from which the ID was obtained.
#[derive(Clone, Copy, Debug)]
pub struct ValueEdit<'value> {
	/// The target occurrence in that document.
	pub id: ValueId,
	/// The new attribute text, before XML escaping.
	pub value: &'value str,
}

struct Replacement<'value> {
	range: Range<usize>,
	value: &'value str,
	quote: char,
	encoded_len: usize,
}

/// Validates an intended replacement for a projected value's XML attribute.
///
/// Pass a value node from this document. An exact unchanged value remains valid.
/// This checks the attribute's original quote, legal XML characters, and escaped
/// scalar size without allocating replacement text. The caller still needs
/// [`serialize`] to check the complete document and aggregate limits.
///
/// # Errors
///
/// Rejects opaque or unsupported attributes, invalid source spans or characters,
/// and escaped values above the scalar byte limit.
pub fn validate_value_scalar(
	document: &BankDocument,
	node: &ValueNode,
	value: &str,
	limits: Limits,
) -> Result<(), SerializeError> {
	prepare_replacement(document, node, value, limits.scalar_bytes).map(|_| ())
}

/// Exports the original bank with only the specified value attributes replaced.
///
/// This does not apply domain-level value rules or update the stored signature. Existing value
/// IDs remain usable following attribute-only edits. An empty edit list or same-value edits
/// preserve the exact original bytes.
///
/// # Errors
///
/// Returns an error if a target is missing, ambiguous, or repeated in the edit list, an edited
/// string is illegal in XML, or the output exceeds the limits or fails XML revalidation.
pub fn serialize(
	document: &BankDocument,
	edits: &[ValueEdit<'_>],
	limits: Limits,
) -> Result<Vec<u8>, SerializeError> {
	if edits.len() > usize::try_from(limits.nodes).unwrap_or(usize::MAX) {
		return Err(SerializeError::LimitExceeded(LimitKind::Nodes));
	}
	let mut pending = BTreeMap::new();
	for edit in edits {
		if pending.insert(edit.id, edit.value).is_some() {
			return Err(SerializeError::DuplicateEdit(edit.id));
		}
	}

	let mut replacements = Vec::new();
	let mut remaining_source_bytes = document.source.len();
	let mut total_replacement_bytes = 0usize;
	if !pending.is_empty() {
		let values = document
			.sections()
			.iter()
			.flat_map(|section| section.keys())
			.flat_map(|key| key.values());
		for node in values {
			let Some(new_value) = pending.remove(&node.id()) else {
				continue;
			};
			let Some(replacement) =
				prepare_replacement(document, node, new_value, limits.scalar_bytes)?
			else {
				continue;
			};
			total_replacement_bytes = total_replacement_bytes
				.checked_add(replacement.encoded_len)
				.filter(|&total| total <= limits.document_bytes)
				.ok_or(SerializeError::LimitExceeded(LimitKind::DocumentBytes))?;
			remaining_source_bytes = remaining_source_bytes
				.checked_sub(replacement.range.len())
				.ok_or(SerializeError::InvalidSourceSpan)?;
			replacements.push(replacement);
		}
	}
	if let Some((&id, _)) = pending.first_key_value() {
		return Err(SerializeError::UnknownValue(id));
	}

	let output_len = remaining_source_bytes
		.checked_add(total_replacement_bytes)
		.filter(|&total| total <= limits.document_bytes)
		.ok_or(SerializeError::LimitExceeded(LimitKind::DocumentBytes))?;
	if replacements.is_empty() {
		if document.limits != limits {
			parse(document.source(), limits).map_err(SerializeError::InvalidOutput)?;
		}
		return Ok(document.source().to_vec());
	}

	let output = replace(&document.source, &replacements, output_len)?;
	let bytes = native_bytes(document, output, limits)?;
	parse(&bytes, limits).map_err(SerializeError::InvalidOutput)?;
	Ok(bytes)
}

fn prepare_replacement<'value>(
	document: &BankDocument,
	node: &ValueNode,
	value: &'value str,
	maximum: usize,
) -> Result<Option<Replacement<'value>>, SerializeError> {
	let attribute = node
		.attribute()
		.ok_or(SerializeError::UnsupportedValue(node.id()))?;
	if value == attribute.value() {
		return Ok(None);
	}
	if matches!(attribute.kind(), ValueKind::Unknown(_)) {
		return Err(SerializeError::UnsupportedValue(node.id()));
	}

	let range = document
		.text_range(attribute.source_range())
		.ok_or(SerializeError::InvalidSourceSpan)?;
	let quote = source_quote(&document.source, &range)?;
	let encoded_len = encoded_length(value, quote, node.id(), maximum)?;
	Ok(Some(Replacement {
		range,
		value,
		quote,
		encoded_len,
	}))
}

pub(crate) fn native_bytes(
	document: &BankDocument,
	output: String,
	limits: Limits,
) -> Result<Vec<u8>, SerializeError> {
	document
		.encode_owned_xml_text(output, limits)
		.map_err(|error| match error.kind() {
			ParseErrorKind::LimitExceeded(limit) => SerializeError::LimitExceeded(limit),
			_ => SerializeError::InvalidOutput(error),
		})
}

pub(crate) fn source_quote(source: &str, range: &Range<usize>) -> Result<char, SerializeError> {
	let quote_start = range
		.start
		.checked_sub(1)
		.ok_or(SerializeError::InvalidSourceSpan)?;
	if source.get(range.clone()).is_none() {
		return Err(SerializeError::InvalidSourceSpan);
	}
	match source.as_bytes().get(quote_start) {
		Some(&quote @ (b'\'' | b'"')) if source.as_bytes().get(range.end) == Some(&quote) => {
			Ok(char::from(quote))
		}
		_ => Err(SerializeError::InvalidSourceSpan),
	}
}

fn encoded_length(
	value: &str,
	quote: char,
	id: ValueId,
	maximum: usize,
) -> Result<usize, SerializeError> {
	if value.len() > maximum {
		return Err(SerializeError::LimitExceeded(LimitKind::ScalarBytes));
	}
	let mut length = 0usize;
	for character in value.chars() {
		if !is_xml_character(character) {
			return Err(SerializeError::InvalidCharacter { id, character });
		}
		let width = escape(character, quote).map_or_else(|| character.len_utf8(), str::len);
		length = length
			.checked_add(width)
			.filter(|&total| total <= maximum)
			.ok_or(SerializeError::LimitExceeded(LimitKind::ScalarBytes))?;
	}
	Ok(length)
}

fn replace(
	source: &str,
	replacements: &[Replacement<'_>],
	capacity: usize,
) -> Result<String, SerializeError> {
	let mut output = String::with_capacity(capacity);
	let mut cursor = 0;
	for replacement in replacements {
		let untouched = source
			.get(cursor..replacement.range.start)
			.ok_or(SerializeError::InvalidSourceSpan)?;
		output.push_str(untouched);
		for character in replacement.value.chars() {
			if let Some(entity) = escape(character, replacement.quote) {
				output.push_str(entity);
			} else {
				output.push(character);
			}
		}
		cursor = replacement.range.end;
	}
	output.push_str(
		source
			.get(cursor..)
			.ok_or(SerializeError::InvalidSourceSpan)?,
	);
	Ok(output)
}

pub(crate) fn escape(character: char, quote: char) -> Option<&'static str> {
	match character {
		'&' => Some("&amp;"),
		'<' => Some("&lt;"),
		'>' => Some("&gt;"),
		'\'' if quote == '\'' => Some("&apos;"),
		'"' if quote == '"' => Some("&quot;"),
		// XML normalizes literal attribute whitespace to spaces, but preserves references.
		'\t' => Some("&#x9;"),
		'\n' => Some("&#xA;"),
		'\r' => Some("&#xD;"),
		_ => None,
	}
}
