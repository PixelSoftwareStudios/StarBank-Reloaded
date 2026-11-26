use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// The version of a document within one open session.
///
/// Display and parsing use canonical decimal so the full range survives transport
/// through clients that cannot represent every 64-bit integer.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(u64);

impl Revision {
	/// The revision of a newly opened document.
	pub const INITIAL: Self = Self(0);

	/// Advances the revision, returning `None` when the counter is exhausted.
	pub fn checked_next(self) -> Option<Self> {
		self.0.checked_add(1).map(Self)
	}
}

impl fmt::Display for Revision {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(formatter)
	}
}

impl FromStr for Revision {
	type Err = ParseRevisionError;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		if value.is_empty()
			|| (value.len() > 1 && value.starts_with('0'))
			|| !value.bytes().all(|byte| byte.is_ascii_digit())
		{
			return Err(ParseRevisionError::NonCanonical);
		}

		value
			.parse::<u64>()
			.map(Self)
			.map_err(|_| ParseRevisionError::Overflow)
	}
}

/// The reason a wire revision could not be decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseRevisionError {
	/// The value is empty or contains a sign, leading zero or non-ASCII digit.
	NonCanonical,
	/// The value exceeds the largest supported counter.
	Overflow,
}

impl fmt::Display for ParseRevisionError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::NonCanonical => {
				formatter.write_str("revision must be a canonical decimal integer")
			}
			Self::Overflow => formatter.write_str("revision exceeds the supported counter range"),
		}
	}
}

impl Error for ParseRevisionError {}
