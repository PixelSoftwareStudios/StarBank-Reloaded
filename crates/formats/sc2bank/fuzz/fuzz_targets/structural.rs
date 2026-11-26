#![no_main]

use std::{ops::Range, sync::OnceLock};

use libfuzzer_sys::fuzz_target;
use sc2bank::{
	BankDocument, Limits, ParseErrorKind, StructuralEdit, StructuralEditError, ValueKind,
	XmlEncoding, apply_structural_edit, parse, serialize,
};

#[path = "structural/identity.rs"]
mod identity;

use identity::{check_addition, check_remap};

const MAX_INPUT_BYTES: usize = 4096;
const MAX_EDIT_BYTES: usize = 96;
const OPERATION_COUNT: u8 = 10;
const LIMITS: Limits = Limits {
	document_bytes: 16 * 1024,
	element_depth: 12,
	nodes: 256,
	attributes_per_element: 16,
	scalar_bytes: 12 * 1024,
	markup_markers: 2048,
};
const ENCODINGS: [XmlEncoding; 3] = [
	XmlEncoding::Utf8,
	XmlEncoding::Utf16Le,
	XmlEncoding::Utf16Be,
];
const UTF16: [XmlEncoding; 2] = [XmlEncoding::Utf16Le, XmlEncoding::Utf16Be];
const KINDS: [ValueKind; 7] = [
	ValueKind::Fixed,
	ValueKind::Flag,
	ValueKind::Int,
	ValueKind::Point,
	ValueKind::String,
	ValueKind::Text,
	ValueKind::Unit,
];

// Distinct lexical anchors intentionally share decoded occurrence names.
const SECTION_OPEN: &str = "<Section name = 'S'>";
const EMPTY_SECTION: &str = "<Section name='S' />";
const KEY_OPEN: &str = "<Key name = 'K'>";
const EMPTY_KEY: &str = "<Key name='K' />";
const OPEN_VALUE: &str = "<Item string = 'é &amp; &#x41;' >";
const CLOSE_VALUE: &str = "</Item  >";
const OPAQUE_COMMENT: &str = "<!-- keep </Item> and 雪🙂 -->";
const SIMPLE_VALUE: &str = "<Item fixed='1.000'/>";
const OPAQUE_CHILD: &str = "<x:Extension><![CDATA[raw <tag> & keep]]></x:Extension>";
const SIGNATURE: &str = "<Signature value = 'old' />";
const EMPTY_BANK: &str = "<Bank version='1' />";

struct Fixture {
	source: String,
	bytes: Vec<u8>,
	document: BankDocument,
	section: String,
	key: String,
	value: String,
}

static FIXTURES: OnceLock<[Fixture; 3]> = OnceLock::new();

struct EditCase<'edit> {
	edit: StructuralEdit<'edit>,
	expected: String,
	removed: Option<Range<usize>>,
}

fuzz_target!(|data: &[u8]| {
	if data.len() > MAX_INPUT_BYTES {
		return;
	}
	let [encoding_selector, operation, flags, tail @ ..] = data else {
		return;
	};
	let encoding_index = usize::from(*encoding_selector) % ENCODINGS.len();
	let encoding = ENCODINGS[encoding_index];
	let fixtures = FIXTURES.get_or_init(|| ENCODINGS.map(fixture));
	let fixture = &fixtures[encoding_index];
	let text = valid_xml_text(tail);
	let value_name = format!("Value_{flags:02x}雪");
	let secondary = flags & 1 != 0;
	let flavor = *flags >> 1;
	let operation = operation % OPERATION_COUNT;

	if operation == 0 && secondary {
		check_empty_bank_addition(encoding, &text);
	} else {
		let case = select_edit(fixture, operation, secondary, flavor, &text, &value_name);
		check_edit(&fixture.document, &fixture.bytes, case, encoding);
	}

	if flavor & 1 != 0 {
		check_malformed_utf16(flavor >> 2);
	}
	if flavor & 2 != 0 {
		check_invalid_structure(fixture);
	}
});

fn fixture(encoding: XmlEncoding) -> Fixture {
	let value = format!("{OPEN_VALUE}{OPAQUE_COMMENT}{CLOSE_VALUE}");
	let key = format!("{KEY_OPEN}{value}{SIMPLE_VALUE}</Key>");
	let section = format!("{SECTION_OPEN}{key}{EMPTY_KEY}{OPAQUE_CHILD}</Section>");
	let body = format!(
		"<Bank xmlns:x='urn:fixture'>\r\n{section}<!-- keep -->{EMPTY_SECTION}{SIGNATURE}</Bank>"
	);
	let source = xml_with_body(&body, encoding);
	let bytes = encode(&source, encoding);
	let document = parse(&bytes, LIMITS).expect("authored structural fixture parses");
	Fixture {
		source,
		bytes,
		document,
		section,
		key,
		value,
	}
}

fn select_edit<'edit>(
	fixture: &Fixture,
	operation: u8,
	secondary: bool,
	flavor: u8,
	text: &'edit str,
	value_name: &'edit str,
) -> EditCase<'edit> {
	let sections = fixture.document.sections();
	let keys = sections[0].keys();
	let values = keys[0].values();
	let index = usize::from(secondary);
	let section = &sections[index];
	let key = &keys[index];
	let value = &values[index];
	let section_source = if secondary {
		EMPTY_SECTION
	} else {
		&fixture.section
	};
	let key_source = if secondary { EMPTY_KEY } else { &fixture.key };
	let value_source = if secondary {
		SIMPLE_VALUE
	} else {
		&fixture.value
	};
	let mut removed = None;
	let (edit, original, replacement) = match operation {
		0 => (
			StructuralEdit::AddSection { name: text },
			SIGNATURE,
			format!("{}{SIGNATURE}", named_element("Section", text)),
		),
		1 => {
			removed = Some(section.source_range());
			(
				StructuralEdit::RemoveSection { id: section.id() },
				section_source,
				String::new(),
			)
		}
		2 => {
			let opening = if secondary {
				EMPTY_SECTION
			} else {
				SECTION_OPEN
			};
			(
				StructuralEdit::RenameSection {
					id: section.id(),
					name: text,
				},
				opening,
				opening.replace("'S'", &format!("'{}'", escape_attribute(text, '\''))),
			)
		}
		3 => (
			StructuralEdit::AddKey {
				section: section.id(),
				name: text,
			},
			section_source,
			append_child(section_source, "Section", &named_element("Key", text)),
		),
		4 => {
			removed = Some(key.source_range());
			(
				StructuralEdit::RemoveKey { id: key.id() },
				key_source,
				String::new(),
			)
		}
		5 => {
			let opening = if secondary { EMPTY_KEY } else { KEY_OPEN };
			(
				StructuralEdit::RenameKey {
					id: key.id(),
					name: text,
				},
				opening,
				opening.replace("'K'", &format!("'{}'", escape_attribute(text, '\''))),
			)
		}
		6 => {
			let kind = &KINDS[usize::from(flavor) % KINDS.len()];
			let child = format!(
				"<{value_name} {}=\"{}\"/>",
				kind.as_str(),
				escape_attribute(text, '"')
			);
			(
				StructuralEdit::AddValue {
					key: key.id(),
					name: value_name,
					kind,
					value: text,
				},
				key_source,
				append_child(key_source, "Key", &child),
			)
		}
		7 => {
			removed = Some(value.source_range());
			(
				StructuralEdit::RemoveValue { id: value.id() },
				value_source,
				String::new(),
			)
		}
		8 => {
			let replacement = if secondary {
				format!("<{value_name} fixed='1.000'/>")
			} else {
				format!(
					"<{value_name} string = 'é &amp; &#x41;' >{OPAQUE_COMMENT}</{value_name}  >"
				)
			};
			(
				StructuralEdit::RenameValue {
					id: value.id(),
					name: value_name,
				},
				value_source,
				replacement,
			)
		}
		9 => {
			let kind = &KINDS[usize::from(flavor) % KINDS.len()];
			let (decoded, lexical, original) = if secondary {
				("1.000", "1.000", SIMPLE_VALUE)
			} else {
				("é & A", "é &amp; &#x41;", OPEN_VALUE)
			};
			let text = if flavor & 4 == 0 { text } else { decoded };
			let escaped = if text == decoded {
				lexical.to_owned()
			} else {
				escape_attribute(text, '\'')
			};
			let new_xml = if secondary {
				format!("<Item {}='{escaped}'/>", kind.as_str())
			} else {
				format!("<Item {} = '{escaped}' >", kind.as_str())
			};
			(
				StructuralEdit::SetValueType {
					id: value.id(),
					kind,
					value: text,
				},
				original,
				new_xml,
			)
		}
		_ => unreachable!("operation selector is bounded"),
	};
	assert_eq!(fixture.source.matches(original).count(), 1);
	let expected = fixture.source.replacen(original, &replacement, 1);
	EditCase {
		edit,
		expected,
		removed,
	}
}

fn named_element(tag: &str, decoded_name: &str) -> String {
	format!("<{tag} name=\"{}\"/>", escape_attribute(decoded_name, '"'))
}

fn append_child(original: &str, tag: &str, child: &str) -> String {
	if let Some(start) = original.strip_suffix("/>") {
		format!("{start}>{child}</{tag}>")
	} else {
		let close = format!("</{tag}>");
		let start = original
			.strip_suffix(&close)
			.expect("authored explicit closer");
		format!("{start}{child}{close}")
	}
}

fn escape_attribute(text: &str, quote: char) -> String {
	let mut output = String::new();
	for character in text.chars() {
		output.push_str(match character {
			'&' => "&amp;",
			'<' => "&lt;",
			'>' => "&gt;",
			'\'' if quote == '\'' => "&apos;",
			'"' if quote == '"' => "&quot;",
			'\t' => "&#x9;",
			'\n' => "&#xA;",
			'\r' => "&#xD;",
			_ => {
				output.push(character);
				continue;
			}
		});
	}
	output
}

fn valid_xml_text(data: &[u8]) -> String {
	String::from_utf8_lossy(&data[..data.len().min(MAX_EDIT_BYTES)])
		.chars()
		.filter(|&character| matches!(
			character,
			'\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
		))
		.collect()
}

fn xml_with_body(body: &str, encoding: XmlEncoding) -> String {
	let label = if encoding == XmlEncoding::Utf8 {
		"UTF-8"
	} else {
		"UTF-16"
	};
	format!(
		"\u{feff}<?xml version='1.0' encoding='{label}'?>\r\n<!-- original 雪 -->{body}<!-- tail -->"
	)
}

fn encode(text: &str, encoding: XmlEncoding) -> Vec<u8> {
	match encoding {
		XmlEncoding::Utf8 => text.as_bytes().to_vec(),
		XmlEncoding::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
		XmlEncoding::Utf16Be => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
	}
}

fn check_empty_bank_addition(encoding: XmlEncoding, section: &str) {
	let xml = xml_with_body(EMPTY_BANK, encoding);
	let bytes = encode(&xml, encoding);
	let document = parse(&bytes, LIMITS).expect("authored empty bank parses");
	let child = named_element("Section", section);
	let expected = xml.replace(EMPTY_BANK, &append_child(EMPTY_BANK, "Bank", &child));
	check_edit(
		&document,
		&bytes,
		EditCase {
			edit: StructuralEdit::AddSection { name: section },
			expected,
			removed: None,
		},
		encoding,
	);
}

fn check_edit(bank: &BankDocument, source: &[u8], case: EditCase<'_>, encoding: XmlEncoding) {
	let result = apply_structural_edit(bank, case.edit, LIMITS)
		.expect("bounded valid structural edit succeeds");
	assert_eq!(bank.source(), source, "input bank is unchanged");
	assert_eq!(
		result.document.source(),
		encode(&case.expected, encoding),
		"exact native bytes match the authored edit expectation for {:?}",
		case.edit,
	);
	assert_eq!(result.document.encoding(), encoding);
	assert_eq!(
		serialize(&result.document, &[], LIMITS).expect("edited bank exports unchanged"),
		result.document.source(),
	);
	let reopened = parse(result.document.source(), LIMITS).expect("edited native bank reparses");
	assert_eq!(reopened.xml_text(), case.expected);
	check_addition(case.edit, &result);
	check_remap(bank, &result, case.removed);
}

fn check_invalid_structure(fixture: &Fixture) {
	let value = &fixture.document.sections()[0].keys()[0].values()[0];
	assert!(matches!(
		apply_structural_edit(
			&fixture.document,
			StructuralEdit::RenameValue {
				id: value.id(),
				name: "bad:prefix"
			},
			LIMITS,
		),
		Err(StructuralEditError::InvalidElementName)
	));
	assert_eq!(fixture.document.source(), fixture.bytes);
}

fn check_malformed_utf16(selector: u8) {
	for encoding in UTF16 {
		let xml = xml_with_body(EMPTY_BANK, encoding);
		let mut bytes = encode(&xml, encoding);
		let expected = match selector % 4 {
			0 => {
				bytes.pop().expect("fixture has a final UTF-16 code unit");
				ParseErrorKind::InvalidXml
			}
			1 | 2 => {
				// The lone surrogate follows the BOM and precedes the declaration's ASCII '<'.
				let surrogate =
					if selector % 4 == 1 { 0xd800 } else { 0xdc00 } + u16::from(selector);
				let octets = match encoding {
					XmlEncoding::Utf16Le => surrogate.to_le_bytes(),
					XmlEncoding::Utf16Be => surrogate.to_be_bytes(),
					XmlEncoding::Utf8 => unreachable!("malformed UTF-16 test selector"),
				};
				let after_bom = size_of::<u16>();
				drop(bytes.splice(after_bom..after_bom, octets));
				ParseErrorKind::InvalidXml
			}
			_ => {
				bytes = encode(
					&xml.replace("encoding='UTF-16'", "encoding='UTF-8'"),
					encoding,
				);
				ParseErrorKind::UnsupportedEncoding
			}
		};
		let error = parse(&bytes, LIMITS).expect_err("malformed UTF-16 must not become editable");
		assert_eq!(error.kind(), expected);
	}
}
