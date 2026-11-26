use std::ops::Range;

use crate::{
	LimitKind, Limits,
	parse::{is_xml_character, is_xml_space},
	serialize::{escape, source_quote},
};

use super::StructuralEditError;

const EMPTY_TAG_END: &str = "/>";
const TAG_END: char = '>';
const TAG_OPEN: &str = "<";
const CLOSE_TAG_OPEN: &str = "</";
const ATTRIBUTE_QUOTE: char = '"';
const NAME_ATTRIBUTE: &str = "name";

pub(super) struct Patch {
	pub range: Range<usize>,
	pub replacement: String,
}

impl Patch {
	pub(super) fn new(range: Range<usize>, replacement: String) -> Self {
		Self { range, replacement }
	}
}

pub(super) fn materialize(
	source: &str,
	patches: &[Patch],
	limits: Limits,
) -> Result<String, StructuralEditError> {
	let mut start = 0;
	let mut removed = 0usize;
	let mut inserted = 0usize;
	for patch in patches {
		if patch.range.start < start || source.get(patch.range.clone()).is_none() {
			return Err(StructuralEditError::InvalidSourceSpan);
		}
		start = patch.range.end;
		removed = removed
			.checked_add(patch.range.len())
			.ok_or(StructuralEditError::InvalidSourceSpan)?;
		inserted = inserted
			.checked_add(patch.replacement.len())
			.ok_or(StructuralEditError::LimitExceeded(LimitKind::DocumentBytes))?;
	}
	let length = source
		.len()
		.checked_sub(removed)
		.and_then(|remaining| remaining.checked_add(inserted))
		.filter(|&length| length <= limits.document_bytes)
		.ok_or(StructuralEditError::LimitExceeded(LimitKind::DocumentBytes))?;

	let mut output = String::with_capacity(length);
	let mut cursor = 0;
	for patch in patches {
		output.push_str(&source[cursor..patch.range.start]);
		output.push_str(&patch.replacement);
		cursor = patch.range.end;
	}
	output.push_str(&source[cursor..]);
	Ok(output)
}

pub(super) fn append_child(
	source: &str,
	parent: Range<usize>,
	parent_name: &str,
	child: String,
) -> Result<(Patch, usize), StructuralEditError> {
	if is_empty_tag(source, parent.clone())? {
		let terminal = parent
			.end
			.checked_sub(EMPTY_TAG_END.len())
			.ok_or(StructuralEditError::InvalidSourceSpan)?;
		let replacement = format!("{TAG_END}{child}{CLOSE_TAG_OPEN}{parent_name}{TAG_END}");
		let child_start = terminal + TAG_END.len_utf8();
		return Ok((Patch::new(terminal..parent.end, replacement), child_start));
	}

	let name = closing_name(source, parent, parent_name)?;
	let position = name
		.start
		.checked_sub(CLOSE_TAG_OPEN.len())
		.ok_or(StructuralEditError::InvalidSourceSpan)?;
	Ok((Patch::new(position..position, child), position))
}

pub(super) fn rename_attribute(
	source: &str,
	element: Range<usize>,
	tag_name: &str,
	current: Option<&str>,
	content: Option<Range<usize>>,
	new: &str,
	limits: Limits,
) -> Result<Vec<Patch>, StructuralEditError> {
	if current == Some(new) {
		return Ok(Vec::new());
	}

	if current.is_some() {
		let content = content.ok_or(StructuralEditError::InvalidSourceSpan)?;
		return Ok(vec![replace_attribute_content(
			source, content, new, limits,
		)?]);
	}

	let end = opening_name(source, element, tag_name)?.end;
	let escaped = encode_attribute(new, ATTRIBUTE_QUOTE, limits)?;
	Ok(vec![Patch::new(
		end..end,
		format!(" {NAME_ATTRIBUTE}={ATTRIBUTE_QUOTE}{escaped}{ATTRIBUTE_QUOTE}"),
	)])
}

pub(super) fn replace_attribute_content(
	source: &str,
	range: Range<usize>,
	value: &str,
	limits: Limits,
) -> Result<Patch, StructuralEditError> {
	let quote = source_quote(source, &range).map_err(|_| StructuralEditError::InvalidSourceSpan)?;
	let escaped = encode_attribute(value, quote, limits)?;
	Ok(Patch::new(range, escaped))
}

pub(super) fn new_named_child(
	tag_name: &str,
	name: &str,
	limits: Limits,
) -> Result<String, StructuralEditError> {
	let name = encode_attribute(name, ATTRIBUTE_QUOTE, limits)?;
	Ok(format!(
		"{TAG_OPEN}{tag_name} {NAME_ATTRIBUTE}={ATTRIBUTE_QUOTE}{name}{ATTRIBUTE_QUOTE}{EMPTY_TAG_END}"
	))
}

pub(super) fn new_value_child(
	name: &str,
	kind: &str,
	value: &str,
	limits: Limits,
) -> Result<String, StructuralEditError> {
	validate_element_name(name, limits)?;
	let value = encode_attribute(value, ATTRIBUTE_QUOTE, limits)?;
	Ok(format!(
		"{TAG_OPEN}{name} {kind}={ATTRIBUTE_QUOTE}{value}{ATTRIBUTE_QUOTE}{EMPTY_TAG_END}"
	))
}

pub(super) fn opening_name(
	source: &str,
	element: Range<usize>,
	name: &str,
) -> Result<Range<usize>, StructuralEditError> {
	let tag = source
		.get(element.clone())
		.and_then(|raw| raw.strip_prefix(TAG_OPEN))
		.and_then(|raw| raw.strip_prefix(name))
		.filter(|tail| tail.starts_with(is_xml_space) || tail.starts_with(['/', TAG_END]))
		.ok_or(StructuralEditError::InvalidSourceSpan)?;
	let end = element.end - tag.len();
	Ok(end - name.len()..end)
}

pub(super) fn closing_name(
	source: &str,
	element: Range<usize>,
	name: &str,
) -> Result<Range<usize>, StructuralEditError> {
	let raw = source
		.get(element.clone())
		.ok_or(StructuralEditError::InvalidSourceSpan)?;
	let relative_close = raw
		.rfind(CLOSE_TAG_OPEN)
		.ok_or(StructuralEditError::InvalidSourceSpan)?;
	let start = element.start + relative_close + CLOSE_TAG_OPEN.len();
	let tail = source
		.get(start..element.end)
		.and_then(|tail| tail.strip_prefix(name))
		.and_then(|tail| tail.strip_suffix(TAG_END))
		.ok_or(StructuralEditError::InvalidSourceSpan)?;
	if !tail.chars().all(is_xml_space) {
		return Err(StructuralEditError::InvalidSourceSpan);
	}
	Ok(start..start + name.len())
}

pub(super) fn is_empty_tag(
	source: &str,
	element: Range<usize>,
) -> Result<bool, StructuralEditError> {
	source
		.get(element)
		.map(|raw| raw.ends_with(EMPTY_TAG_END))
		.ok_or(StructuralEditError::InvalidSourceSpan)
}

pub(super) fn validate_element_name(name: &str, limits: Limits) -> Result<(), StructuralEditError> {
	if name.len() > limits.scalar_bytes {
		return Err(StructuralEditError::LimitExceeded(LimitKind::ScalarBytes));
	}
	if name.len() > limits.document_bytes {
		return Err(StructuralEditError::LimitExceeded(LimitKind::DocumentBytes));
	}
	let mut characters = name.chars();
	if !characters.next().is_some_and(is_name_start) || !characters.all(is_name_character) {
		return Err(StructuralEditError::InvalidElementName);
	}
	Ok(())
}

fn is_name_start(character: char) -> bool {
	// XML 1.0 Fifth Edition NameStartChar, excluding the namespace separator.
	matches!(character,
		'A'..='Z' | 'a'..='z' | '_' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
		| '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
		| '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
		| '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
		| '\u{10000}'..='\u{EFFFF}')
}

fn is_name_character(character: char) -> bool {
	is_name_start(character)
		|| matches!(character, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

fn encode_attribute(
	value: &str,
	quote: char,
	limits: Limits,
) -> Result<String, StructuralEditError> {
	check_attribute_size(value.len(), limits)?;
	let mut output = String::with_capacity(value.len());
	for character in value.chars() {
		if !is_xml_character(character) {
			return Err(StructuralEditError::InvalidCharacter(character));
		}
		let escaped = escape(character, quote);
		let width = escaped.map_or_else(|| character.len_utf8(), str::len);
		let length = output
			.len()
			.checked_add(width)
			.ok_or(StructuralEditError::LimitExceeded(LimitKind::ScalarBytes))?;
		check_attribute_size(length, limits)?;
		if let Some(escaped) = escaped {
			output.push_str(escaped);
		} else {
			output.push(character);
		}
	}
	Ok(output)
}

fn check_attribute_size(length: usize, limits: Limits) -> Result<(), StructuralEditError> {
	if length > limits.scalar_bytes {
		return Err(StructuralEditError::LimitExceeded(LimitKind::ScalarBytes));
	}
	if length > limits.document_bytes {
		return Err(StructuralEditError::LimitExceeded(LimitKind::DocumentBytes));
	}
	Ok(())
}
