use std::{iter, ops::Range, str};

use roxmltree::{Document, Node, ParsingOptions, TextPos};

use crate::{
	BankDocument, Key, KeyId, LimitKind, Limits, Location, OpaqueReason, ParseError,
	ParseErrorKind, Section, SectionId, StoredSignature, TypedAttribute, ValueId, ValueKind,
	ValueNode, preflight,
};

const BANK: &str = "Bank";
const SECTION: &str = "Section";
const KEY: &str = "Key";
const SIGNATURE: &str = "Signature";
const VERSION: &str = "version";
const NAME: &str = "name";
const VALUE: &str = "value";
const PI_OPENING: &str = "<?";
const PI_CLOSING: &str = "?>";

/// Parses XML 1.0 bank bytes as UTF-8, preserving the source exactly.
///
/// Unknown markup remains in the source. Only unqualified, direct bank, section and key
/// elements enter the structural projection. Repeated occurrences keep distinct IDs.
///
/// # Errors
///
/// Returns a structured error for unsupported encodings or declarations, malformed XML,
/// duplicate signature elements, a wrong root, or an exceeded resource limit.
pub fn parse(bytes: &[u8], limits: Limits) -> Result<BankDocument, ParseError> {
	if bytes.len() > limits.document_bytes {
		return Err(preflight::limit_error(LimitKind::DocumentBytes));
	}
	let source = str::from_utf8(bytes).map_err(|error| {
		ParseError::new(ParseErrorKind::UnsupportedEncoding).with_detail(error.to_string())
	})?;
	let inspection = preflight::inspect(source, limits)?;
	let options = ParsingOptions {
		allow_dtd: false,
		// The validator also counts its synthetic document root.
		nodes_limit: limits.nodes.saturating_add(1),
		entity_resolver: None,
	};
	let xml =
		Document::parse_with_options(source, options).map_err(|error| xml_error(source, error))?;
	check_validated_nodes(source, &xml, limits, inspection.declaration_start)?;
	project(
		source,
		xml.root_element(),
		&inspection.namespace_declarations,
		limits,
	)
}

fn xml_error(source: &str, error: roxmltree::Error) -> ParseError {
	match error {
		roxmltree::Error::DtdDetected => ParseError::new(ParseErrorKind::Doctype),
		roxmltree::Error::NodesLimitReached => preflight::limit_error(LimitKind::Nodes),
		_ => {
			let position = error.pos();
			let location = validator_byte(source, position).map_or(
				Location {
					byte: None,
					line: position.row,
					column: position.col,
				},
				|byte| Location::at(source, byte),
			);
			let detail = error.to_string().replace(&format!(" at {position}"), "");
			ParseError::new(ParseErrorKind::InvalidXml)
				.with_detail(detail)
				.at(location)
		}
	}
}

fn validator_byte(source: &str, position: TextPos) -> Option<usize> {
	// The validator counts only LF as a line break. Recover its byte offset before
	// reporting the location with the editor's XML line-ending rules.
	let row = usize::try_from(position.row.checked_sub(1)?).ok()?;
	let column = usize::try_from(position.col.checked_sub(1)?).ok()?;
	let mut lines = source.split('\n');
	let line_start: usize = lines.by_ref().take(row).map(|line| line.len() + 1).sum();
	let line = lines.next()?;
	let column_byte = line
		.char_indices()
		.map(|(offset, _)| offset)
		.chain(iter::once(line.len()))
		.nth(column)?;
	Some(line_start + column_byte)
}

fn check_validated_nodes(
	source: &str,
	xml: &Document<'_>,
	limits: Limits,
	declaration_start: Option<usize>,
) -> Result<(), ParseError> {
	for node in xml.descendants() {
		if let Some(instruction) = node.pi() {
			// The validator exposes a declaration with non-space XML whitespace as a PI.
			// The preflight separately validated its complete declaration grammar.
			if Some(node.range().start) != declaration_start {
				check_processing_instruction(source, node.range(), instruction.target)?;
			}
		} else if node.is_text()
			&& node
				.text()
				.is_some_and(|text| text.len() > limits.scalar_bytes)
		{
			// Adjacent character data, CDATA and references can form a single decoded string.
			return Err(preflight::limit_error(LimitKind::ScalarBytes)
				.at(Location::at(source, node.range().start)));
		}
	}
	Ok(())
}

fn check_processing_instruction(
	source: &str,
	range: Range<usize>,
	target: &str,
) -> Result<(), ParseError> {
	let location = || Location::at(source, range.start);
	if target.eq_ignore_ascii_case(preflight::XML_TARGET) {
		return Err(
			preflight::invalid_xml("reserved XML processing instruction target").at(location()),
		);
	}
	let remainder = source
		.get(range.clone())
		.and_then(|instruction| instruction.strip_prefix(PI_OPENING))
		.and_then(|instruction| instruction.strip_prefix(target));

	// The validator identifies the XML Name but also accepts adjacent non-Name data.
	if !remainder.is_some_and(|data| data.starts_with(PI_CLOSING) || data.starts_with(is_xml_space))
	{
		return Err(preflight::invalid_xml(
			"processing instruction data requires intervening whitespace",
		)
		.at(location()));
	}
	Ok(())
}

fn project(
	source: &str,
	root: Node<'_, '_>,
	namespace_declarations: &[usize],
	limits: Limits,
) -> Result<BankDocument, ParseError> {
	if !is_unqualified(root, BANK) {
		return Err(
			ParseError::new(ParseErrorKind::WrongRoot).at(Location::at(source, root.range().start))
		);
	}

	let mut signature = None;
	let mut sections = Vec::new();
	let mut has_unprojected_bank_nodes = false;
	for child in root.children() {
		if is_unqualified(child, SECTION) {
			let section = project_section(
				source,
				child,
				namespace_declarations,
				&mut has_unprojected_bank_nodes,
			);
			sections.push(section);
		} else if is_unqualified(child, SIGNATURE) {
			if signature.is_some() {
				return Err(ParseError::new(ParseErrorKind::DuplicateSignature)
					.at(Location::at(source, child.range().start)));
			}
			has_unprojected_bank_nodes |= child.children().any(is_significant);
			signature = Some(StoredSignature {
				value: decoded_attribute(child, VALUE),
				source_range: child.range(),
			});
		} else {
			has_unprojected_bank_nodes |= is_significant(child);
		}
	}

	Ok(BankDocument {
		source: source.to_owned(),
		limits,
		version: decoded_attribute(root, VERSION),
		signature,
		sections,
		has_unprojected_bank_nodes,
	})
}

fn project_section(
	source: &str,
	section: Node<'_, '_>,
	namespace_declarations: &[usize],
	has_unprojected_bank_nodes: &mut bool,
) -> Section {
	let mut keys = Vec::new();
	for child in section.children() {
		if !is_unqualified(child, KEY) {
			*has_unprojected_bank_nodes |= is_significant(child);
			continue;
		}

		let mut values = Vec::new();
		for element in child.children() {
			if element.is_element() {
				values.push(project_value(source, element, namespace_declarations));
			} else {
				*has_unprojected_bank_nodes |= is_significant(element);
			}
		}
		keys.push(Key {
			id: KeyId(child.id().get()),
			name: decoded_attribute(child, NAME),
			source_range: child.range(),
			values,
		});
	}
	Section {
		id: SectionId(section.id().get()),
		name: decoded_attribute(section, NAME),
		source_range: section.range(),
		keys,
	}
}

fn project_value(
	source: &str,
	element: Node<'_, '_>,
	namespace_declarations: &[usize],
) -> ValueNode {
	let namespace = element.tag_name().namespace();
	let range = element.range();
	let declares_namespace = namespace_declarations.binary_search(&range.start).is_ok();
	ValueNode {
		id: ValueId(element.id().get()),
		name: element.tag_name().name().to_owned(),
		namespace: namespace.map(str::to_owned),
		source_range: range,
		value: project_type_attribute(source, element, namespace.is_some() || declares_namespace),
	}
}

fn project_type_attribute(
	source: &str,
	element: Node<'_, '_>,
	has_namespace: bool,
) -> Result<TypedAttribute, OpaqueReason> {
	if has_namespace {
		return Err(OpaqueReason::Namespace);
	}
	if element.children().any(is_significant) {
		return Err(OpaqueReason::StructuredContent);
	}

	let mut attributes = element.attributes();
	let attribute = attributes.next().ok_or(OpaqueReason::MissingType)?;
	if attributes.next().is_some() {
		return Err(OpaqueReason::AmbiguousType);
	}
	if attribute.namespace().is_some() {
		return Err(OpaqueReason::Namespace);
	}
	let source_range =
		attribute_value_range(source, attribute.range()).ok_or(OpaqueReason::InvalidSourceSpan)?;
	Ok(TypedAttribute {
		kind: ValueKind::from_name(attribute.name()),
		value: attribute.value().to_owned(),
		source_range,
	})
}

fn attribute_value_range(source: &str, range: Range<usize>) -> Option<Range<usize>> {
	// Derive offsets from the complete attribute range. Sub-ranges reported by the XML
	// validator can truncate very long attribute names or whitespace around the equals sign.
	let original = source.get(range.clone())?;
	let (_, after_equals) = original.split_once('=')?;
	let quoted = after_equals.trim_start_matches(is_xml_space);
	let quote = *quoted.as_bytes().first()?;
	if quoted.len() < 2
		|| !matches!(quote, b'\'' | b'"')
		|| quoted.as_bytes().last() != Some(&quote)
	{
		return None;
	}
	let start = range.end.checked_sub(quoted.len())?.checked_add(1)?;
	let end = range.end.checked_sub(1)?;
	source.get(start..end).map(|_| start..end)
}

fn decoded_attribute(node: Node<'_, '_>, name: &str) -> Option<String> {
	node.attributes()
		.find(|attribute| attribute.namespace().is_none() && attribute.name() == name)
		.map(|attribute| attribute.value().to_owned())
}

fn is_unqualified(node: Node<'_, '_>, name: &str) -> bool {
	node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

fn is_significant(node: Node<'_, '_>) -> bool {
	node.is_element()
		|| node.is_text()
			&& node
				.text()
				.is_some_and(|text| !text.chars().all(is_xml_space))
}

pub(crate) fn is_xml_space(character: char) -> bool {
	matches!(character, ' ' | '\t' | '\r' | '\n')
}

pub(crate) fn is_xml_character(character: char) -> bool {
	matches!(
		character,
		'\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
	)
}
