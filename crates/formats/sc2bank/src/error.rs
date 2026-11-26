use std::{error::Error, fmt};

use crate::{LimitKind, ValueId};

/// Position of a diagnostic. Byte offsets are zero-based, lines and columns are one-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
	/// Offset in the original bytes, when reported by the reader.
	pub byte: Option<usize>,
	/// XML line number in the original source.
	pub line: u32,
	/// Character column in the original source.
	pub column: u32,
}

impl Location {
	pub(crate) fn at(source: &str, byte: usize) -> Self {
		let end = source.floor_char_boundary(byte.min(source.len()));
		let mut line: u32 = 1;
		let mut column: u32 = 1;
		let mut previous_cr = false;
		for character in source[..end].chars() {
			match character {
				'\r' => {
					line = line.saturating_add(1);
					column = 1;
				}
				'\n' if previous_cr => {}
				'\n' => {
					line = line.saturating_add(1);
					column = 1;
				}
				_ => column = column.saturating_add(1),
			}
			previous_cr = character == '\r';
		}
		Self {
			byte: Some(byte),
			line,
			column,
		}
	}
}

/// Category of failure from the bank reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
	/// Source bytes or encoding declaration use an encoding other than UTF-8.
	UnsupportedEncoding,
	/// The declaration specifies an XML version other than 1.0.
	UnsupportedXmlVersion,
	/// Malformed XML, including malformed encoding declarations.
	InvalidXml,
	/// Document type declarations are prohibited.
	Doctype,
	/// The XML root is not an unqualified Bank.
	WrongRoot,
	/// More than one direct unqualified Signature is present.
	DuplicateSignature,
	/// The resource or input size exceeds the selected bound.
	LimitExceeded(LimitKind),
}

/// Structured failure to read bank XML.
#[derive(Debug)]
pub struct ParseError {
	kind: ParseErrorKind,
	location: Option<Location>,
	detail: Option<String>,
}

impl ParseError {
	pub(crate) fn new(kind: ParseErrorKind) -> Self {
		Self {
			kind,
			location: None,
			detail: None,
		}
	}

	pub(crate) fn at(mut self, location: Location) -> Self {
		self.location = Some(location);
		self
	}

	pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
		self.detail = Some(detail.into());
		self
	}

	/// Returns the failure category.
	pub fn kind(&self) -> ParseErrorKind {
		self.kind
	}

	/// Returns the original source location when available.
	pub fn location(&self) -> Option<Location> {
		self.location
	}
}

impl fmt::Display for ParseError {
	fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self.kind {
			ParseErrorKind::UnsupportedEncoding => write!(output, "expected UTF-8 bank XML")?,
			ParseErrorKind::UnsupportedXmlVersion => write!(output, "expected XML version 1.0")?,
			ParseErrorKind::InvalidXml => write!(output, "invalid XML")?,
			ParseErrorKind::Doctype => {
				write!(output, "XML document type declarations are unsupported")?
			}
			ParseErrorKind::WrongRoot => {
				write!(output, "expected an unqualified Bank root element")?
			}
			ParseErrorKind::DuplicateSignature => write!(output, "multiple Signature elements")?,
			ParseErrorKind::LimitExceeded(limit) => {
				write!(output, "XML limit exceeded ({limit:?})")?
			}
		}
		if let Some(location) = self.location {
			write!(output, " at {}:{}", location.line, location.column)?;
		}
		if let Some(detail) = &self.detail {
			write!(output, " ({detail})")?;
		}
		Ok(())
	}
}

impl Error for ParseError {}

/// Failure to construct a validated lossless export.
#[derive(Debug)]
#[non_exhaustive]
pub enum SerializeError {
	/// The value ID does not occur in this document.
	UnknownValue(ValueId),
	/// The same value ID is edited more than once.
	DuplicateEdit(ValueId),
	/// The value is opaque or has an unrecognized type.
	UnsupportedValue(ValueId),
	/// The proposed value contains a prohibited XML 1.0 character.
	InvalidCharacter {
		/// Edited child value.
		id: ValueId,
		/// Prohibited Unicode scalar.
		character: char,
	},
	/// The proposed signature is not an uppercase hexadecimal digest.
	InvalidSignature,
	/// The output exceeds a configured limit.
	LimitExceeded(LimitKind),
	/// The source span cannot safely be replaced.
	InvalidSourceSpan,
	/// The output could not be parsed with the same format contract.
	InvalidOutput(ParseError),
}

impl fmt::Display for SerializeError {
	fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnknownValue(id) => write!(output, "unknown value {}", id.get()),
			Self::DuplicateEdit(id) => write!(output, "duplicate edit for value {}", id.get()),
			Self::UnsupportedValue(id) => write!(output, "value {} has no editable type", id.get()),
			Self::InvalidCharacter { id, character } => write!(
				output,
				"value {} contains prohibited XML character U+{:04X}",
				id.get(),
				u32::from(*character),
			),
			Self::InvalidSignature => write!(output, "expected an uppercase hexadecimal signature"),
			Self::LimitExceeded(limit) => write!(output, "XML output limit exceeded ({limit:?})"),
			Self::InvalidSourceSpan => write!(output, "unsafe source span for XML edit"),
			Self::InvalidOutput(error) => {
				write!(output, "could not validate edited bank ({error})")
			}
		}
	}
}

impl Error for SerializeError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::InvalidOutput(error) => Some(error),
			_ => None,
		}
	}
}
