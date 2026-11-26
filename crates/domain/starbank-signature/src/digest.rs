use std::{fmt, str::FromStr};

const DIGEST_BYTES: usize = 20;
const HEX_LENGTH: usize = DIGEST_BYTES * 2;

/// A legacy SC2 signature digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Signature([u8; DIGEST_BYTES]);

impl Signature {
	pub(crate) fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
		Self(bytes)
	}

	/// Returns the digest bytes.
	pub fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
		&self.0
	}
}

impl fmt::Display for Signature {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(&hex::encode_upper(self.0))
	}
}

impl FromStr for Signature {
	type Err = ParseSignatureError;

	/// Parses forty ASCII hex digits, accepting either letter case.
	fn from_str(value: &str) -> Result<Self, Self::Err> {
		if value.len() != HEX_LENGTH {
			return Err(ParseSignatureError::Length);
		}
		let mut bytes = [0; DIGEST_BYTES];
		hex::decode_to_slice(value, &mut bytes).map_err(|_| ParseSignatureError::NonHex)?;
		Ok(Self(bytes))
	}
}

/// The stored signature text does not encode a SHA-1 digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseSignatureError {
	/// The digest text has the wrong byte length.
	Length,
	/// At least one byte is outside the ASCII hexadecimal alphabet.
	NonHex,
}

impl fmt::Display for ParseSignatureError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Length => write!(formatter, "signature must contain {HEX_LENGTH} hex digits"),
			Self::NonHex => formatter.write_str("signature contains a non-hex digit"),
		}
	}
}

impl std::error::Error for ParseSignatureError {}
