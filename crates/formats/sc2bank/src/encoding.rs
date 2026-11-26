use std::str;

use crate::{Location, ParseError, ParseErrorKind, preflight};

pub(crate) const UTF8_BOM: char = '\u{feff}';
const UTF8_NAME: &str = "UTF-8";
const MAX_ENCODING_NAME_IN_DIAGNOSTIC: usize = 64;

/// Returns validated UTF-8 without allocating or replacing source bytes.
pub(crate) fn utf8_source(bytes: &[u8]) -> Result<&str, ParseError> {
	if let Some(encoding) = unsupported_signature(bytes) {
		return Err(ParseError::new(ParseErrorKind::UnsupportedEncoding)
			.with_detail(format!("detected {encoding} input"))
			.at(Location {
				byte: Some(0),
				line: 1,
				column: 1,
			}));
	}
	str::from_utf8(bytes).map_err(|error| {
		let mut failure =
			ParseError::new(ParseErrorKind::UnsupportedEncoding).with_detail(error.to_string());
		let byte = error.valid_up_to();
		if let Ok(prefix) = str::from_utf8(&bytes[..byte]) {
			if let Some(declaration) = preflight::probe_unsupported_declaration(prefix) {
				return declaration;
			}
			failure = failure.at(Location::at(prefix, byte));
		}
		failure
	})
}

pub(crate) fn validate_name(name: &str) -> Result<(), ParseError> {
	let mut bytes = name.bytes();
	if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
		|| !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
	{
		return Err(preflight::invalid_xml("malformed XML encoding name"));
	}
	if name.eq_ignore_ascii_case(UTF8_NAME) {
		return Ok(());
	}

	let shown = name.get(..MAX_ENCODING_NAME_IN_DIAGNOSTIC).unwrap_or(name);
	let suffix = if shown.len() == name.len() { "" } else { "…" };
	Err(ParseError::new(ParseErrorKind::UnsupportedEncoding)
		.with_detail(format!("declared encoding {shown}{suffix}")))
}

fn unsupported_signature(bytes: &[u8]) -> Option<&'static str> {
	// W3C XML 1.0 Appendix F identifies families from the initial bytes. Check
	// four-octet signatures before the overlapping two-octet UTF-16 BOMs.
	let name = match bytes {
		[0, 0, 0xfe, 0xff, ..] | [0, 0, 0, b'<', ..] => "UTF-32BE",
		[0xff, 0xfe, 0, 0, ..] | [b'<', 0, 0, 0, ..] => "UTF-32LE",
		[0, 0, 0xff, 0xfe, ..] | [0, 0, b'<', 0, ..] => "UCS-4 (2143 byte order)",
		[0xfe, 0xff, 0, 0, ..] | [0, b'<', 0, 0, ..] => "UCS-4 (3412 byte order)",
		[0xfe, 0xff, ..] | [0, b'<', 0, b'?', ..] => "UTF-16BE",
		[0xff, 0xfe, ..] | [b'<', 0, b'?', 0, ..] => "UTF-16LE",
		[0x4c, 0x6f, 0xa7, 0x94, ..] => "EBCDIC-family XML",
		_ => return None,
	};
	Some(name)
}
