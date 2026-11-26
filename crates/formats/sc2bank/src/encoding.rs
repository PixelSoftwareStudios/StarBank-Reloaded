use std::{borrow::Cow, str};

use crate::{LimitKind, Limits, Location, ParseError, ParseErrorKind, preflight};

pub(crate) const UTF8_BOM: char = '\u{feff}';
const UTF16_LE_BOM: [u8; 2] = [0xff, 0xfe];
const UTF16_BE_BOM: [u8; 2] = [0xfe, 0xff];
pub(crate) const UTF16_UNIT_BYTES: usize = size_of::<u16>();
const UTF16_NAME: &str = "UTF-16";
const MAX_ENCODING_NAME_IN_DIAGNOSTIC: usize = 64;

/// Native encoding of the validated bank XML.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XmlEncoding {
	/// UTF-8, with or without a byte-order mark.
	Utf8,
	/// Little-endian UTF-16 with a matching byte-order mark.
	Utf16Le,
	/// Big-endian UTF-16 with a matching byte-order mark.
	Utf16Be,
}

impl XmlEncoding {
	/// Canonical lowercase name of the native byte encoding.
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Utf8 => "utf-8",
			Self::Utf16Le => "utf-16le",
			Self::Utf16Be => "utf-16be",
		}
	}
}

pub(crate) fn decode(
	bytes: &[u8],
	limits: Limits,
) -> Result<(Cow<'_, str>, XmlEncoding), ParseError> {
	if let Some(encoding) = unsupported_signature(bytes) {
		return Err(ParseError::new(ParseErrorKind::UnsupportedEncoding)
			.with_detail(format!("detected {encoding} input"))
			.at(Location {
				byte: Some(0),
				line: 1,
				column: 1,
			}));
	}

	match bytes {
		[0xff, 0xfe, ..] => decode_utf16(bytes, XmlEncoding::Utf16Le, u16::from_le_bytes, limits),
		[0xfe, 0xff, ..] => decode_utf16(bytes, XmlEncoding::Utf16Be, u16::from_be_bytes, limits),
		_ => Ok((Cow::Borrowed(utf8_source(bytes)?), XmlEncoding::Utf8)),
	}
}

fn decode_utf16(
	bytes: &[u8],
	encoding: XmlEncoding,
	read_unit: fn([u8; UTF16_UNIT_BYTES]) -> u16,
	limits: Limits,
) -> Result<(Cow<'_, str>, XmlEncoding), ParseError> {
	let (pairs, partial) = bytes.as_chunks::<UTF16_UNIT_BYTES>();
	let units = pairs.iter().copied().map(read_unit);
	let mut text = String::with_capacity(bytes.len());
	let mut byte = 0;
	for scalar in char::decode_utf16(units) {
		let character =
			scalar.map_err(|_| invalid_utf16(&text, byte, "unpaired UTF-16 surrogate"))?;
		let length = text
			.len()
			.checked_add(character.len_utf8())
			.ok_or_else(|| preflight::limit_error(LimitKind::DocumentBytes))?;
		if length > limits.document_bytes {
			return Err(preflight::limit_error(LimitKind::DocumentBytes));
		}
		text.push(character);
		byte += character.len_utf16() * UTF16_UNIT_BYTES;
	}
	if !partial.is_empty() {
		return Err(invalid_utf16(&text, byte, "partial UTF-16 code unit"));
	}
	Ok((Cow::Owned(text), encoding))
}

fn invalid_utf16(prefix: &str, byte: usize, detail: &str) -> ParseError {
	let mut location = Location::at(prefix, prefix.len());
	location.byte = Some(byte);
	preflight::invalid_xml(detail).at(location)
}

pub(crate) fn encode(
	text: &str,
	encoding: XmlEncoding,
	limits: Limits,
) -> Result<Vec<u8>, ParseError> {
	if text.len() > limits.document_bytes {
		return Err(preflight::limit_error(LimitKind::DocumentBytes));
	}
	let (bom, to_bytes): (_, fn(u16) -> [u8; 2]) = match encoding {
		XmlEncoding::Utf8 => return Ok(text.as_bytes().to_vec()),
		XmlEncoding::Utf16Le => (UTF16_LE_BOM, u16::to_le_bytes),
		XmlEncoding::Utf16Be => (UTF16_BE_BOM, u16::to_be_bytes),
	};
	let text_has_bom = text.starts_with(UTF8_BOM);
	let body = text.strip_prefix(UTF8_BOM).unwrap_or(text);
	let length = body
		.encode_utf16()
		.count()
		.checked_mul(UTF16_UNIT_BYTES)
		.and_then(|length| length.checked_add(bom.len()))
		.filter(|&length| length <= limits.document_bytes)
		.ok_or_else(|| preflight::limit_error(LimitKind::DocumentBytes))?;
	// The mandatory native BOM contributes a UTF-8 marker to the next working view.
	if !text_has_bom && text.len().saturating_add(UTF8_BOM.len_utf8()) > limits.document_bytes {
		return Err(preflight::limit_error(LimitKind::DocumentBytes));
	}
	let mut bytes = Vec::with_capacity(length);
	bytes.extend_from_slice(&bom);
	bytes.extend(body.encode_utf16().flat_map(to_bytes));
	Ok(bytes)
}

fn utf8_source(bytes: &[u8]) -> Result<&str, ParseError> {
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

pub(crate) fn validate_name(name: &str, encoding: XmlEncoding) -> Result<(), ParseError> {
	let mut bytes = name.bytes();
	if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
		|| !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
	{
		return Err(preflight::invalid_xml("malformed XML encoding name"));
	}
	if name.eq_ignore_ascii_case(encoding.as_str())
		|| (encoding != XmlEncoding::Utf8 && name.eq_ignore_ascii_case(UTF16_NAME))
	{
		return Ok(());
	}

	let shown = name.get(..MAX_ENCODING_NAME_IN_DIAGNOSTIC).unwrap_or(name);
	let suffix = if shown.len() == name.len() { "" } else { "…" };
	Err(
		ParseError::new(ParseErrorKind::UnsupportedEncoding).with_detail(format!(
			"declared encoding {shown}{suffix} conflicts with {} input",
			encoding.as_str(),
		)),
	)
}

fn unsupported_signature(bytes: &[u8]) -> Option<&'static str> {
	// W3C XML 1.0 Appendix F identifies families from the initial bytes. Check
	// four-octet signatures before the overlapping two-octet UTF-16 BOMs.
	let name = match bytes {
		[0, 0, 0xfe, 0xff, ..] | [0, 0, 0, b'<', ..] => "UTF-32BE",
		[0xff, 0xfe, 0, 0, ..] | [b'<', 0, 0, 0, ..] => "UTF-32LE",
		[0, 0, 0xff, 0xfe, ..] | [0, 0, b'<', 0, ..] => "UCS-4 (2143 byte order)",
		[0xfe, 0xff, 0, 0, ..] | [0, b'<', 0, 0, ..] => "UCS-4 (3412 byte order)",
		[0, b'<', 0, b'?', ..] => "UTF-16BE without a byte-order mark",
		[b'<', 0, b'?', 0, ..] => "UTF-16LE without a byte-order mark",
		[0x4c, 0x6f, 0xa7, 0x94, ..] => "EBCDIC-family XML",
		_ => return None,
	};
	Some(name)
}
