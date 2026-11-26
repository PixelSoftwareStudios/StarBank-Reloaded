use std::{error::Error, fmt};

use sc2bank::{BankDocument, Limits, SerializeError, replace_signature};

use crate::{
	ComputeSignatureError, SignatureStatus, SigningContext, compute_signature, verify_signature,
};

/// Failure to prepare a verified signed XML document.
#[derive(Debug)]
#[non_exhaustive]
pub enum ResignError {
	/// The source document could not be canonicalized.
	Compute(ComputeSignatureError),
	/// The signature edit could not be serialized and reparsed.
	Serialize(SerializeError),
	/// The reparsed document did not verify with the supplied context.
	Verification(SignatureStatus),
}

impl fmt::Display for ResignError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Compute(error) => write!(formatter, "could not compute signature ({error})"),
			Self::Serialize(error) => write!(formatter, "could not write signature ({error})"),
			Self::Verification(status) => {
				write!(formatter, "prepared bank did not verify ({status:?})")
			}
		}
	}
}

impl Error for ResignError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Compute(error) => Some(error),
			Self::Serialize(error) => Some(error),
			Self::Verification(_) => None,
		}
	}
}

/// Prepares a signed bank in memory, leaving the input document unchanged.
///
/// The context describes the intended destination, which may differ from the
/// original bank. The result is the reparsed XML with a verified signature.
///
/// # Errors
///
/// Returns an error if the input does not have a supported canonical form, the
/// signature edit cannot be safely serialized, or the reparsed result fails
/// verification.
pub fn resign(
	document: &BankDocument,
	context: &SigningContext,
	limits: Limits,
) -> Result<BankDocument, ResignError> {
	let digest = compute_signature(document, context).map_err(ResignError::Compute)?;
	let prepared =
		replace_signature(document, &digest.to_string(), limits).map_err(ResignError::Serialize)?;
	let status = verify_signature(&prepared, Some(context));
	if status != SignatureStatus::Valid {
		return Err(ResignError::Verification(status));
	}
	Ok(prepared)
}
