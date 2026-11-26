use serde::Serialize;

use crate::{EditorError, LexemeError};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ProtocolError {
	InvalidProfile,
	InvalidProfileIdentity,
	ProfileContextRequired,
	ProfileUnavailable,
	InvalidProfileValue,
	ProfileArithmeticOverflow,
	ProfileReadOnly,
	StaleProfilePreview,
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
	RevertUnavailable,
	DuplicateOccurrence,
	OpaqueValue,
	InvalidInteger,
	InvalidFixed,
	InvalidRawNumber,
	RawEncodingMismatch,
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
			Self::InvalidProfileIdentity => {
				"The supplied profile identity is invalid or conflicts with the open bank."
			}
			Self::InvalidProfile => "The file is not a supported local bank profile.",
			Self::ProfileContextRequired => {
				"Set the bank’s author and filename context before importing a legacy profile."
			}
			Self::ProfileUnavailable => "The selected profile no longer matches this bank.",
			Self::InvalidProfileValue => {
				"A value falls outside the profile’s declared mapping or bounds."
			}
			Self::ProfileArithmeticOverflow => {
				"A dependent value exceeds the profile’s exact numeric range."
			}
			Self::ProfileReadOnly => "The profile calculates this field from other values.",
			Self::StaleProfilePreview => {
				"The profile edit preview is outdated. Preview the current change again."
			}
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
			Self::RevertUnavailable => "The selected occurrence has no matching opening content.",
			Self::DuplicateOccurrence => {
				"An edit transaction cannot change an occurrence more than once."
			}
			Self::OpaqueValue => "This value must be edited in Raw View.",
			Self::InvalidInteger => "Enter an integer with optional comma grouping.",
			Self::InvalidFixed => "Enter an exact decimal with optional comma grouping.",
			Self::RawEncodingMismatch => "Raw XML must preserve the bank's original encoding.",
			Self::InvalidRawNumber => "Use ungrouped integer or exact decimal text in Raw View.",
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
			EditorError::UnknownValue(_)
			| EditorError::UnknownKey(_)
			| EditorError::UnknownSection(_) => Self::InvalidOccurrence,
			EditorError::RevertUnavailable(_) => Self::RevertUnavailable,
			EditorError::DuplicateValue(_) => Self::DuplicateOccurrence,
			EditorError::OpaqueValue(_) => Self::OpaqueValue,
			EditorError::InvalidValue { source, .. }
			| EditorError::InvalidAddedValue { source, .. } => match source {
				LexemeError::InvalidInteger => Self::InvalidInteger,
				LexemeError::InvalidFixed => Self::InvalidFixed,
				LexemeError::UnsupportedKind => Self::UnsupportedValueKind,
			},
			EditorError::InvalidRawNumber { .. } => Self::InvalidRawNumber,
			EditorError::RawEncodingMismatch => Self::RawEncodingMismatch,
			EditorError::NoHistory => Self::NoHistory,
			EditorError::EditLimit => Self::EditLimit,
			_ => fallback,
		}
	}
}
