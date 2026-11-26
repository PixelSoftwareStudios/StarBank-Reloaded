use std::fmt;

use sc2bank::{ParseError, SerializeError};
use starbank_core::Revision;

use crate::{LexemeError, OccurrenceId};

/// A rejected editor operation. The current document remains unchanged.
#[derive(Debug)]
#[non_exhaustive]
pub enum EditorError {
	/// The caller must refresh its projection before attempting the operation.
	StaleRevision {
		/// Revision supplied by the caller.
		expected: Revision,
		/// Current session revision.
		current: Revision,
	},
	/// The document reached the largest supported revision.
	RevisionExhausted,
	/// The session cannot allocate another occurrence ID.
	IdentityExhausted,
	/// The identifier does not name a value in the current document.
	UnknownValue(OccurrenceId),
	/// The same value appeared twice in one transaction.
	DuplicateValue(OccurrenceId),
	/// A value is opaque and can only be changed through raw XML.
	OpaqueValue(OccurrenceId),
	/// An input value or transaction exceeds the configured limits.
	EditLimit,
	/// An intended structured value failed validation.
	InvalidValue {
		/// Target occurrence.
		id: OccurrenceId,
		/// Rejected lexical form.
		source: LexemeError,
	},
	/// The submitted XML could not replace or open a document.
	Parse(ParseError),
	/// A requested change could not be serialized losslessly.
	Serialize(SerializeError),
	/// The requested history direction has no available entry.
	NoHistory,
	/// A history change does not match the current source.
	InconsistentHistory,
	/// The requested export could not be signed.
	Signing(starbank_signature::ResignError),
	/// The save receipt belongs to a different open session.
	WrongSession,
}

impl fmt::Display for EditorError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::StaleRevision { expected, current } => write!(
				formatter,
				"expected revision {expected}, current revision {current}"
			),
			Self::RevisionExhausted => formatter.write_str("document revision exhausted"),
			Self::IdentityExhausted => formatter.write_str("document occurrence IDs exhausted"),
			Self::UnknownValue(id) => write!(formatter, "unknown value {id}"),
			Self::DuplicateValue(id) => write!(formatter, "duplicate edit for value {id}"),
			Self::OpaqueValue(id) => write!(formatter, "value {id} requires raw XML editing"),
			Self::EditLimit => formatter.write_str("edit exceeds document limits"),
			Self::InvalidValue { id, source } => write!(formatter, "invalid value {id}: {source}"),
			Self::Parse(source) => source.fmt(formatter),
			Self::Serialize(source) => source.fmt(formatter),
			Self::NoHistory => formatter.write_str("no history entry in the requested direction"),
			Self::InconsistentHistory => {
				formatter.write_str("current document does not match its history")
			}
			Self::Signing(source) => source.fmt(formatter),
			Self::WrongSession => {
				formatter.write_str("prepared export belongs to another document")
			}
		}
	}
}

impl std::error::Error for EditorError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::InvalidValue { source, .. } => Some(source),
			Self::Parse(source) => Some(source),
			Self::Serialize(source) => Some(source),
			Self::Signing(source) => Some(source),
			_ => None,
		}
	}
}
