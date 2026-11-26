use std::{borrow::Cow, ops::Range};

use crate::{
	BankDocument, LimitKind, Limits, SerializeError, parse,
	parse::{BANK, SIGNATURE, VALUE},
	serialize::native_bytes,
};

const SIGNATURE_HEX_DIGITS: usize = 40;
const EMPTY_TAG_END: &str = "/>";
const CLOSE_TAG_START: &str = "</";

/// Replaces or inserts the direct unqualified signature value and reparses the result.
///
/// Replacing the existing value preserves its surrounding quotes and the rest of
/// the element. A missing signature is appended inside the root. The original
/// document is unchanged. This validates XML only, independent of signing context.
///
/// # Errors
///
/// Returns an error for invalid signature text, an unsafe source span, or output
/// that exceeds the limits or fails validation.
pub fn replace_signature(
	document: &BankDocument,
	digest: &str,
	limits: Limits,
) -> Result<BankDocument, SerializeError> {
	if digest.len() != SIGNATURE_HEX_DIGITS
		|| !digest
			.bytes()
			.all(|byte| byte.is_ascii_digit() || matches!(byte, b'A'..=b'F'))
	{
		return Err(SerializeError::InvalidSignature);
	}

	let (range, replacement) = signature_patch(document, digest)?;
	let source = &document.source;
	if source.get(range.clone()).is_none() {
		return Err(SerializeError::InvalidSourceSpan);
	}
	let output_len = source
		.len()
		.checked_sub(range.len())
		.and_then(|remaining| remaining.checked_add(replacement.len()))
		.filter(|&length| length <= limits.document_bytes)
		.ok_or(SerializeError::LimitExceeded(LimitKind::DocumentBytes))?;

	let mut output = String::with_capacity(output_len);
	output.push_str(&source[..range.start]);
	output.push_str(&replacement);
	output.push_str(&source[range.end..]);
	let bytes = native_bytes(document, output, limits)?;
	parse(&bytes, limits).map_err(SerializeError::InvalidOutput)
}

fn signature_patch<'digest>(
	document: &BankDocument,
	digest: &'digest str,
) -> Result<(Range<usize>, Cow<'digest, str>), SerializeError> {
	if let Some(signature) = &document.signature {
		if signature.value.is_some() {
			let range = signature
				.value_range
				.clone()
				.ok_or(SerializeError::InvalidSourceSpan)?;
			return Ok((range, Cow::Borrowed(digest)));
		}
		let range = document
			.text_range(signature.source_range())
			.ok_or(SerializeError::InvalidSourceSpan)?;
		let offset = range.start + 1 + SIGNATURE.len();
		return Ok((offset..offset, Cow::Owned(format!(" {VALUE}=\"{digest}\""))));
	}

	let root = document
		.source
		.get(document.root_range.clone())
		.ok_or(SerializeError::InvalidSourceSpan)?;
	let element = format!("<{SIGNATURE} {VALUE}=\"{digest}\"{EMPTY_TAG_END}");
	if let Some(opening) = root.strip_suffix(EMPTY_TAG_END) {
		let offset = document.root_range.start + opening.len();
		return Ok((
			offset..document.root_range.end,
			Cow::Owned(format!(">{element}{CLOSE_TAG_START}{BANK}>")),
		));
	}
	let offset = root
		.rfind(CLOSE_TAG_START)
		.ok_or(SerializeError::InvalidSourceSpan)?;
	let offset = document.root_range.start + offset;
	Ok((offset..offset, Cow::Owned(element)))
}
