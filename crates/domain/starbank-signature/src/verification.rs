use sc2bank::BankDocument;

use crate::{
	ComputeSignatureError, ParseSignatureError, Signature, SigningContext, compute_signature,
};

/// Why a present signature element could not be read as a digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MalformedSignature {
	/// The signature element lacks its value attribute.
	MissingValue,
	/// Its value attribute does not encode an ASCII hex digest.
	InvalidDigest(ParseSignatureError),
}

/// Verification outcome for the stored signature and supplied context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignatureStatus {
	/// No signature element is present.
	Unsigned,
	/// A signature element is present but malformed.
	Malformed(MalformedSignature),
	/// A signature is present but the signing context is unavailable.
	MissingContext,
	/// The projected XML cannot safely be canonicalized.
	Unsupported(ComputeSignatureError),
	/// The stored digest matches the computed one.
	Valid,
	/// The stored digest differs from the computed one for this context.
	Invalid,
}

/// Checks the stored digest against an explicitly supplied signing context.
///
/// Absence or invalid digest syntax is reported even when context is unavailable.
/// A digest comparison requires valid syntax, context and supported XML.
#[must_use]
pub fn verify_signature(
	document: &BankDocument,
	context: Option<&SigningContext>,
) -> SignatureStatus {
	let Some(stored) = document.signature() else {
		return SignatureStatus::Unsigned;
	};
	let Some(value) = stored.value() else {
		return SignatureStatus::Malformed(MalformedSignature::MissingValue);
	};
	let parsed: Signature = match value.parse() {
		Ok(parsed) => parsed,
		Err(error) => return SignatureStatus::Malformed(MalformedSignature::InvalidDigest(error)),
	};
	let Some(context) = context else {
		return SignatureStatus::MissingContext;
	};
	match compute_signature(document, context) {
		Ok(computed) if computed == parsed => SignatureStatus::Valid,
		Ok(_) => SignatureStatus::Invalid,
		Err(error) => SignatureStatus::Unsupported(error),
	}
}
