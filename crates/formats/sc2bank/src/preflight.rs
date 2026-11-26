use quick_xml::{
	Reader,
	escape::resolve_predefined_entity,
	events::{BytesDecl, BytesRef, BytesStart, Event},
};

use crate::{
	LimitKind, Limits, Location, ParseError, ParseErrorKind,
	encoding::{self, UTF8_BOM},
	limits::{MAX_NAMESPACE_DECLARATIONS, MAX_NAMESPACE_SCOPE},
	parse::{is_xml_character, is_xml_space},
};

pub(crate) const XML_TARGET: &str = "xml";
const XML_VERSION: &str = "1.0";
const VERSION_FIELD: &str = "version";
const ENCODING_FIELD: &str = "encoding";
const STANDALONE_FIELD: &str = "standalone";
const XMLNS: &str = "xmlns";
const MAX_DECLARATION_PROBE_BYTES: usize = 2 * 1024;

#[derive(Default)]
pub(crate) struct Inspection {
	pub namespace_declarations: Vec<usize>,
	pub declaration_start: Option<usize>,
}

/// Scans without a DOM so resource limits apply before tree allocation.
pub(crate) fn inspect(source: &str, limits: Limits) -> Result<Inspection, ParseError> {
	check_marker_budget(source, limits.markup_markers)?;

	let xml = source.strip_prefix(UTF8_BOM).unwrap_or(source);
	let offset = source.len() - xml.len();
	let mut reader = Reader::from_str(xml);
	reader.config_mut().enable_all_checks(true);
	let mut open_namespace_counts = Vec::new();
	let mut namespace_declarations = 0;
	let mut nodes = 0u32;
	let mut inspection = Inspection::default();

	loop {
		let position = offset.saturating_add(position_in(xml, reader.buffer_position()));
		let location = || Location::at(source, position);
		let event = reader.read_event().map_err(|error| {
			let error_position = offset.saturating_add(position_in(xml, reader.error_position()));
			invalid_xml(error).at(Location::at(source, error_position))
		})?;

		match &event {
			Event::Eof => break,
			Event::End(_) => {
				open_namespace_counts.pop();
				continue;
			}
			Event::DocType(_) => {
				return Err(ParseError::new(ParseErrorKind::Doctype).at(location()));
			}
			Event::Decl(declaration) => {
				if position != offset {
					return Err(ParseError::new(ParseErrorKind::InvalidXml)
						.with_detail("XML declaration must begin the document")
						.at(location()));
				}
				check_scalar(declaration.len(), limits).map_err(|error| error.at(location()))?;
				check_declaration(declaration).map_err(|error| error.at(location()))?;
				inspection.declaration_start = Some(position);
				continue;
			}
			Event::Start(tag) | Event::Empty(tag) => {
				if open_namespace_counts.len() >= limits.element_depth {
					return Err(limit_error(LimitKind::ElementDepth).at(location()));
				}
				let local_count = check_tag(tag, limits).map_err(|error| error.at(location()))?;
				let inherited = open_namespace_counts.last().copied().unwrap_or_default();
				let scope = namespace_count(
					inherited,
					local_count,
					MAX_NAMESPACE_SCOPE,
					LimitKind::NamespaceScope,
				)
				.map_err(|error| error.at(location()))?;
				namespace_declarations = namespace_count(
					namespace_declarations,
					local_count,
					MAX_NAMESPACE_DECLARATIONS,
					LimitKind::NamespaceDeclarations,
				)
				.map_err(|error| error.at(location()))?;
				if matches!(event, Event::Start(_)) {
					open_namespace_counts.push(scope);
				}
				if local_count != 0 {
					inspection.namespace_declarations.push(position);
				}
			}
			Event::PI(instruction) => {
				check_scalar(instruction.len(), limits).map_err(|error| error.at(location()))?;
			}
			Event::Text(text) | Event::Comment(text) => {
				check_scalar(text.len(), limits).map_err(|error| error.at(location()))?;
			}
			Event::CData(text) => {
				check_scalar(text.len(), limits).map_err(|error| error.at(location()))?;
			}
			Event::GeneralRef(reference) => {
				check_scalar(reference.len(), limits).map_err(|error| error.at(location()))?;
				check_reference(reference).map_err(|error| error.at(location()))?;
			}
		}

		if nodes >= limits.nodes {
			return Err(limit_error(LimitKind::Nodes).at(location()));
		}
		nodes += 1;
	}

	Ok(inspection)
}

fn position_in(xml: &str, position: u64) -> usize {
	usize::try_from(position)
		.unwrap_or(xml.len())
		.min(xml.len())
}

fn check_marker_budget(source: &str, maximum: usize) -> Result<(), ParseError> {
	let mut openings = 0;
	let mut equals = 0;
	for (position, byte) in source.bytes().enumerate() {
		let count = match byte {
			b'<' => &mut openings,
			b'=' => &mut equals,
			_ => continue,
		};
		if *count >= maximum {
			return Err(limit_error(LimitKind::MarkupMarkers).at(Location::at(source, position)));
		}
		*count += 1;
	}
	Ok(())
}

fn check_tag(tag: &BytesStart<'_>, limits: Limits) -> Result<usize, ParseError> {
	check_scalar(tag.name().as_ref().len(), limits)?;
	let mut namespace_declarations = 0;
	for (index, attribute) in tag.attributes().enumerate() {
		if index >= limits.attributes_per_element {
			return Err(limit_error(LimitKind::AttributesPerElement));
		}
		let attribute = attribute.map_err(invalid_xml)?;
		let name = attribute.key.as_ref();
		check_scalar(name.len(), limits)?;
		check_scalar(attribute.value.len(), limits)?;
		check_attribute_references(attribute.value.as_ref())?;
		namespace_declarations +=
			usize::from(is_namespace_declaration(name, attribute.value.as_ref())?);
	}
	Ok(namespace_declarations)
}

fn is_namespace_declaration(name: &str, value: &str) -> Result<bool, ParseError> {
	let Some((prefix, local)) = name.split_once(':') else {
		return Ok(name == XMLNS);
	};
	if prefix == XMLNS {
		if value.is_empty() {
			return Err(invalid_xml(
				"a namespace prefix cannot be undeclared in XML 1.0",
			));
		}
		return Ok(true);
	}
	if local == XMLNS {
		// roxmltree 0.21.2 classifies this qualified attribute as an unprefixed
		// declaration, which can silently change the projected bank namespace.
		return Err(ParseError::new(ParseErrorKind::UnsupportedNamespace)
			.with_detail("prefixed xmlns attribute"));
	}
	Ok(false)
}

fn namespace_count(
	previous: usize,
	added: usize,
	maximum: usize,
	kind: LimitKind,
) -> Result<usize, ParseError> {
	previous
		.checked_add(added)
		.filter(|&count| count <= maximum)
		.ok_or_else(|| limit_error(kind))
}

fn check_attribute_references(value: &str) -> Result<(), ParseError> {
	for tail in value.split('&').skip(1) {
		let (token, _) = tail
			.split_once(';')
			.ok_or_else(|| invalid_xml("unterminated entity or character reference"))?;
		check_reference(&BytesRef::new(token))?;
	}
	Ok(())
}

fn check_reference(reference: &BytesRef<'_>) -> Result<(), ParseError> {
	// Require valid XML scalars before the XML validator can normalize a malformed reference.
	match reference.resolve_char_ref().map_err(invalid_xml)? {
		Some(character) if is_xml_character(character) => Ok(()),
		None if resolve_predefined_entity(reference.as_ref()).is_some() => Ok(()),
		_ => Err(invalid_xml("unsupported XML entity or character reference")),
	}
}

/// Reads only a bounded initial declaration after an input fails UTF-8 validation.
pub(crate) fn probe_unsupported_declaration(prefix: &str) -> Option<ParseError> {
	let xml = prefix.strip_prefix(UTF8_BOM).unwrap_or(prefix);
	if !xml.strip_prefix("<?")?.starts_with(XML_TARGET) {
		return None;
	}
	let length = xml.floor_char_boundary(MAX_DECLARATION_PROBE_BYTES.min(xml.len()));
	let Event::Decl(declaration) = Reader::from_str(&xml[..length]).read_event().ok()? else {
		return None;
	};
	let byte = prefix.len() - xml.len();
	check_declaration(&declaration)
		.err()
		.filter(|error| error.kind() == ParseErrorKind::UnsupportedEncoding)
		.map(|error| error.at(Location::at(prefix, byte)))
}

fn check_declaration(declaration: &BytesDecl<'_>) -> Result<(), ParseError> {
	let content: &str = declaration.as_ref();
	let fields = content
		.strip_prefix(XML_TARGET)
		.ok_or_else(|| invalid_xml("invalid XML declaration target"))?;
	let (name, version, mut remaining) = split_declaration_field(fields)?;
	if name != VERSION_FIELD {
		return Err(invalid_xml("XML declaration must start with version"));
	}
	if version != XML_VERSION {
		return Err(ParseError::new(ParseErrorKind::UnsupportedXmlVersion));
	}

	let mut declared_encoding = None;
	let mut has_standalone = false;
	while !remaining.trim_start_matches(is_xml_space).is_empty() {
		let (name, value, tail) = split_declaration_field(remaining)?;
		match name {
			ENCODING_FIELD if declared_encoding.is_none() && !has_standalone => {
				declared_encoding = Some(value);
			}
			STANDALONE_FIELD if !has_standalone && matches!(value, "yes" | "no") => {
				has_standalone = true;
			}
			_ => return Err(invalid_xml("invalid XML declaration attributes")),
		}
		remaining = tail;
	}
	declared_encoding.map_or(Ok(()), encoding::validate_name)
}

fn split_declaration_field(input: &str) -> Result<(&str, &str, &str), ParseError> {
	let field = input.trim_start_matches(is_xml_space);
	if field.len() == input.len() {
		return Err(invalid_xml(
			"XML declaration attributes require intervening whitespace",
		));
	}
	let (name, contents) = field
		.split_once('=')
		.ok_or_else(|| invalid_xml("missing XML declaration attribute"))?;
	let contents = contents.trim_start_matches(is_xml_space);
	let quote = match contents.as_bytes().first() {
		Some(b'\'') => '\'',
		Some(b'"') => '"',
		_ => return Err(invalid_xml("XML declaration attribute must be quoted")),
	};
	let (value, remaining) = contents[1..]
		.split_once(quote)
		.ok_or_else(|| invalid_xml("unterminated XML declaration attribute"))?;
	Ok((name.trim_end_matches(is_xml_space), value, remaining))
}

fn check_scalar(length: usize, limits: Limits) -> Result<(), ParseError> {
	if length > limits.scalar_bytes {
		return Err(limit_error(LimitKind::ScalarBytes));
	}
	Ok(())
}

pub(crate) fn limit_error(limit: LimitKind) -> ParseError {
	ParseError::new(ParseErrorKind::LimitExceeded(limit))
}

pub(crate) fn invalid_xml(detail: impl ToString) -> ParseError {
	ParseError::new(ParseErrorKind::InvalidXml).with_detail(detail.to_string())
}
