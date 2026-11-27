use serde::Serialize;

use crate::{ValueIssue, ValueIssueCode};

use super::error::ProtocolError;

#[derive(Serialize)]
pub(super) struct WireValueIssue {
	id: String,
	code: WireValueIssueCode,
	message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum WireValueIssueCode {
	UnknownValue,
	DuplicateValue,
	OpaqueValue,
	ValueTooLarge,
	InvalidXmlCharacter,
	InvalidInteger,
	InvalidFixed,
	UnsupportedKind,
}

impl From<ValueIssue> for WireValueIssue {
	fn from(issue: ValueIssue) -> Self {
		let (code, message) = match issue.code {
			ValueIssueCode::UnknownValue => (
				WireValueIssueCode::UnknownValue,
				ProtocolError::InvalidOccurrence,
			),
			ValueIssueCode::DuplicateValue => (
				WireValueIssueCode::DuplicateValue,
				ProtocolError::DuplicateOccurrence,
			),
			ValueIssueCode::OpaqueValue => {
				(WireValueIssueCode::OpaqueValue, ProtocolError::OpaqueValue)
			}
			ValueIssueCode::ValueTooLarge => {
				(WireValueIssueCode::ValueTooLarge, ProtocolError::EditLimit)
			}
			ValueIssueCode::InvalidXmlCharacter => (
				WireValueIssueCode::InvalidXmlCharacter,
				ProtocolError::InvalidXmlCharacter,
			),
			ValueIssueCode::InvalidInteger => (
				WireValueIssueCode::InvalidInteger,
				ProtocolError::InvalidInteger,
			),
			ValueIssueCode::InvalidFixed => (
				WireValueIssueCode::InvalidFixed,
				ProtocolError::InvalidFixed,
			),
			ValueIssueCode::UnsupportedKind => (
				WireValueIssueCode::UnsupportedKind,
				ProtocolError::UnsupportedValueKind,
			),
		};
		Self {
			id: issue.id.to_string(),
			code,
			message: message.message(),
		}
	}
}
