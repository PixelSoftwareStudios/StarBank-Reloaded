use serde::Serialize;
use starbank_signature::SignatureStatus;

use super::error::ProtocolError;
use crate::{Session, is_value_editable};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum WireSignatureStatus {
	Unsigned,
	Malformed,
	MissingContext,
	Unsupported,
	Valid,
	Invalid,
}

impl From<SignatureStatus> for WireSignatureStatus {
	fn from(status: SignatureStatus) -> Self {
		match status {
			SignatureStatus::Unsigned => Self::Unsigned,
			SignatureStatus::Malformed(_) => Self::Malformed,
			SignatureStatus::MissingContext => Self::MissingContext,
			SignatureStatus::Valid => Self::Valid,
			SignatureStatus::Invalid => Self::Invalid,
			_ => Self::Unsupported,
		}
	}
}

#[derive(Serialize)]
pub(super) struct Snapshot<'a> {
	dirty: bool,
	can_undo: bool,
	can_redo: bool,
	signature_status: WireSignatureStatus,
	sections: Vec<Section<'a>>,
}

#[derive(Serialize)]
struct Section<'a> {
	id: String,
	name: Option<&'a str>,
	keys: Vec<Key<'a>>,
}

#[derive(Serialize)]
struct Key<'a> {
	id: String,
	name: Option<&'a str>,
	values: Vec<Value<'a>>,
}

#[derive(Serialize)]
struct Value<'a> {
	id: String,
	name: &'a str,
	kind: Option<&'a str>,
	value: Option<&'a str>,
	editable: bool,
}

impl<'a> Snapshot<'a> {
	pub(super) fn from_session(session: &'a Session) -> Result<Self, ProtocolError> {
		let sections = session
			.document()
			.sections()
			.iter()
			.map(|section| Section::from_section(session, section))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			dirty: session.is_dirty(),
			can_undo: session.can_undo(),
			can_redo: session.can_redo(),
			signature_status: session.signature_status().into(),
			sections,
		})
	}
}

impl<'a> Section<'a> {
	fn from_section(
		session: &Session,
		section: &'a sc2bank::Section,
	) -> Result<Self, ProtocolError> {
		Ok(Self {
			id: session
				.id_of_section(section.id())
				.ok_or(ProtocolError::Internal)?
				.to_string(),
			name: section.name(),
			keys: section
				.keys()
				.iter()
				.map(|key| Key::from_key(session, key))
				.collect::<Result<Vec<_>, _>>()?,
		})
	}
}

impl<'a> Key<'a> {
	fn from_key(session: &Session, key: &'a sc2bank::Key) -> Result<Self, ProtocolError> {
		Ok(Self {
			id: session
				.id_of_key(key.id())
				.ok_or(ProtocolError::Internal)?
				.to_string(),
			name: key.name(),
			values: key
				.values()
				.iter()
				.map(|value| Value::from_value(session, value))
				.collect::<Result<Vec<_>, _>>()?,
		})
	}
}

impl<'a> Value<'a> {
	fn from_value(session: &Session, value: &'a sc2bank::ValueNode) -> Result<Self, ProtocolError> {
		let attribute = value.attribute();
		Ok(Self {
			id: session
				.id_of_value(value.id())
				.ok_or(ProtocolError::Internal)?
				.to_string(),
			name: value.name(),
			kind: attribute.map(|typed| typed.kind().as_str()),
			value: attribute.map(sc2bank::TypedAttribute::value),
			editable: attribute.is_some_and(|typed| is_value_editable(typed.kind())),
		})
	}
}
