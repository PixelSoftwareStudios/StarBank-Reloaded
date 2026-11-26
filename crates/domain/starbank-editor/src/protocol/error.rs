use serde::Serialize;

use crate::{EditorError, LexemeError};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ProtocolError {
	InvalidRequest,
	UnsupportedVersion,
	RequestTooLarge,
	MissingAttachment,
	UnexpectedAttachment,
	AttachmentTooLarge,
	InvalidRevision,
	UnknownDocument,
	DirtyDocument,
	StaleRevision,
	InvalidContext,
	InvalidOccurrence,
	DuplicateOccurrence,
	OpaqueValue,
	InvalidInteger,
	InvalidFixed,
	UnsupportedValueKind,
	NoHistory,
	EditLimit,
	EmptyEdit,
	TooManyEdits,
	InvalidDocument,
	InvalidEdit,
	ExportFailed,
	Internal,
}

impl ProtocolError {
	pub(super) fn message(self) -> &'static str {
		match self {
			Self::InvalidRequest => "The request does not match the engine protocol.",
			Self::UnsupportedVersion => {
				"This engine does not support the requested protocol version."
			}
			Self::RequestTooLarge => "The request exceeds the engine control-message size limit.",
			Self::MissingAttachment => "This command requires a bank byte attachment.",
			Self::UnexpectedAttachment => "This command does not accept a byte attachment.",
			Self::AttachmentTooLarge => "The attachment exceeds the engine document size limit.",
			Self::InvalidRevision => "The revision must be a canonical decimal counter.",
			Self::UnknownDocument => "The requested document is not open.",
			Self::DirtyDocument => "The document has unsaved changes.",
			Self::StaleRevision => "The document changed. Refresh its snapshot before retrying.",
			Self::InvalidContext => "The signing context contains an invalid handle or bank name.",
			Self::InvalidOccurrence => {
				"An occurrence ID is invalid or no longer belongs to the document."
			}
			Self::DuplicateOccurrence => {
				"An edit transaction cannot change an occurrence more than once."
			}
			Self::OpaqueValue => "This value must be edited in Raw View.",
			Self::InvalidInteger => "Enter an integer with optional comma grouping.",
			Self::InvalidFixed => "Enter an exact decimal with optional comma grouping.",
			Self::UnsupportedValueKind => "This value type has no supported structured edit.",
			Self::NoHistory => "There is no history entry in the requested direction.",
			Self::EditLimit => "The edit exceeds the engine document limit.",
			Self::EmptyEdit => "An edit transaction must contain at least one value change.",
			Self::TooManyEdits => "The edit transaction exceeds the operation count limit.",
			Self::InvalidDocument => "The bank XML could not be opened safely.",
			Self::InvalidEdit => "The requested change could not be applied.",
			Self::ExportFailed => "The bank output could not be prepared.",
			Self::Internal => "The engine could not complete the request.",
		}
	}

	pub(super) fn editor(error: &EditorError, fallback: Self) -> Self {
		match error {
			EditorError::StaleRevision { .. } => Self::StaleRevision,
			EditorError::UnknownValue(_) => Self::InvalidOccurrence,
			EditorError::DuplicateValue(_) => Self::DuplicateOccurrence,
			EditorError::OpaqueValue(_) => Self::OpaqueValue,
			EditorError::InvalidValue {
				source: LexemeError::InvalidInteger,
				..
			} => Self::InvalidInteger,
			EditorError::InvalidValue {
				source: LexemeError::InvalidFixed,
				..
			} => Self::InvalidFixed,
			EditorError::InvalidValue {
				source: LexemeError::UnsupportedKind,
				..
			} => Self::UnsupportedValueKind,
			EditorError::NoHistory => Self::NoHistory,
			EditorError::EditLimit => Self::EditLimit,
			_ => fallback,
		}
	}
}
