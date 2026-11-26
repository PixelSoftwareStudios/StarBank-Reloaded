use std::fmt;

use sc2bank::{BankDocument, KeyId, OpaqueReason, SectionId, ValueId, ValueKind, ValueNode};
use sha1::{Digest as _, Sha1};

use crate::{Signature, SigningContext};

const NON_ASCII_REPLACEMENT: u8 = b'?';
const ENCODING_BUFFER_BYTES: usize = 256;

/// A bank contains data whose signature cannot be computed safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComputeSignatureError {
	/// Unprojected XML may affect canonical ordering or values.
	UnprojectedNodes,
	/// A section is missing its name attribute.
	MissingSectionName(SectionId),
	/// A key is missing its name attribute.
	MissingKeyName(KeyId),
	/// A child contains unsupported value structure.
	OpaqueValue {
		/// Document-local occurrence with unsupported structure.
		id: ValueId,
		/// Why the XML could not be interpreted as an ordinary value.
		reason: OpaqueReason,
	},
}

impl fmt::Display for ComputeSignatureError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnprojectedNodes => formatter.write_str("bank contains unprojected XML"),
			Self::MissingSectionName(id) => {
				write!(formatter, "missing name on section {}", id.get())
			}
			Self::MissingKeyName(id) => write!(formatter, "missing name on key {}", id.get()),
			Self::OpaqueValue { id, reason } => {
				write!(
					formatter,
					"unsupported XML for value {}: {reason:?}",
					id.get()
				)
			}
		}
	}
}

impl std::error::Error for ComputeSignatureError {}

/// Computes the legacy signature for the bank and exact signing context.
///
/// Orders borrowed occurrences by UTF-16 ordinal names. The parsed document,
/// including its source order, stored signature and original XML, is unchanged.
///
/// # Errors
///
/// Returns an error when the document has unnamed or unprojected structural
/// nodes or a value that cannot be canonicalized safely.
pub fn compute_signature(
	document: &BankDocument,
	context: &SigningContext,
) -> Result<Signature, ComputeSignatureError> {
	if document.has_unprojected_bank_nodes() {
		return Err(ComputeSignatureError::UnprojectedNodes);
	}
	let mut sections = document
		.sections()
		.iter()
		.map(|section| {
			let name = section
				.name()
				.ok_or(ComputeSignatureError::MissingSectionName(section.id()))?;
			Ok((name, section))
		})
		.collect::<Result<Vec<_>, _>>()?;
	sort_ordinal(&mut sections);

	let mut digest = Sha1::new();
	for input in [context.author(), context.player(), context.bank_name()] {
		update_ascii(&mut digest, input);
	}
	for (name, section) in sections {
		update_ascii(&mut digest, name);
		let mut keys = section
			.keys()
			.iter()
			.map(|key| {
				let name = key
					.name()
					.ok_or(ComputeSignatureError::MissingKeyName(key.id()))?;
				Ok((name, key))
			})
			.collect::<Result<Vec<_>, _>>()?;
		sort_ordinal(&mut keys);

		for (name, key) in keys {
			update_ascii(&mut digest, name);
			let mut values = key
				.values()
				.iter()
				.map(|value| (value.name(), value))
				.collect::<Vec<_>>();
			sort_ordinal(&mut values);
			for (name, value) in values {
				update_ascii(&mut digest, name);
				update_value(&mut digest, value)?;
			}
		}
	}
	Ok(Signature::from_bytes(digest.finalize().into()))
}

fn sort_ordinal<T>(values: &mut [(&str, T)]) {
	values.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
}

fn update_value(digest: &mut Sha1, value: &ValueNode) -> Result<(), ComputeSignatureError> {
	if let Some(reason) = value.opaque_reason()
		&& reason != OpaqueReason::MissingType
	{
		return Err(ComputeSignatureError::OpaqueValue {
			id: value.id(),
			reason,
		});
	}
	if let Some(attribute) = value.attribute() {
		update_ascii(digest, attribute.kind().as_str());
		if *attribute.kind() != ValueKind::Text {
			update_ascii(digest, attribute.value());
		}
	}
	Ok(())
}

fn update_ascii(digest: &mut Sha1, value: &str) {
	if value.is_ascii() {
		digest.update(value.as_bytes());
		return;
	}

	// .NET ASCII fallback works per UTF-16 unit, including both parts of a surrogate pair.
	let mut buffer = [0; ENCODING_BUFFER_BYTES];
	let mut filled = 0;
	for unit in value.encode_utf16() {
		buffer[filled] = u8::try_from(unit)
			.ok()
			.filter(u8::is_ascii)
			.unwrap_or(NON_ASCII_REPLACEMENT);
		filled += 1;
		if filled == buffer.len() {
			digest.update(buffer);
			filled = 0;
		}
	}
	digest.update(&buffer[..filled]);
}
