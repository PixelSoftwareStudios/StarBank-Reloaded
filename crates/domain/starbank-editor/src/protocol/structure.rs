use sc2bank::ValueKind;
use serde::Deserialize;

use crate::StructureChange;

use super::error::ProtocolError;
use super::parse_occurrence;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum WireStructure {
	AddSection {
		name: String,
	},
	RemoveSection {
		id: String,
	},
	RenameSection {
		id: String,
		name: String,
	},
	AddKey {
		section_id: String,
		name: String,
	},
	RemoveKey {
		id: String,
	},
	RenameKey {
		id: String,
		name: String,
	},
	AddValue {
		key_id: String,
		name: String,
		value_kind: WireEditKind,
		value: String,
	},
	RemoveValue {
		id: String,
	},
	RenameValue {
		id: String,
		name: String,
	},
	SetValueType {
		id: String,
		value_kind: WireEditKind,
		value: String,
	},
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum WireEditKind {
	Int,
	Fixed,
	String,
	Text,
}

impl WireEditKind {
	fn value_kind(self) -> &'static ValueKind {
		match self {
			Self::Int => &ValueKind::Int,
			Self::Fixed => &ValueKind::Fixed,
			Self::String => &ValueKind::String,
			Self::Text => &ValueKind::Text,
		}
	}
}

impl WireStructure {
	pub(super) fn interpret(&self) -> Result<StructureChange<'_>, ProtocolError> {
		Ok(match self {
			Self::AddSection { name } => StructureChange::AddSection { name },
			Self::RemoveSection { id } => StructureChange::RemoveSection {
				id: parse_occurrence(id)?,
			},
			Self::RenameSection { id, name } => StructureChange::RenameSection {
				id: parse_occurrence(id)?,
				name,
			},
			Self::AddKey { section_id, name } => StructureChange::AddKey {
				section: parse_occurrence(section_id)?,
				name,
			},
			Self::RemoveKey { id } => StructureChange::RemoveKey {
				id: parse_occurrence(id)?,
			},
			Self::RenameKey { id, name } => StructureChange::RenameKey {
				id: parse_occurrence(id)?,
				name,
			},
			Self::AddValue {
				key_id,
				name,
				value_kind,
				value,
			} => StructureChange::AddValue {
				key: parse_occurrence(key_id)?,
				name,
				kind: value_kind.value_kind(),
				value,
			},
			Self::RemoveValue { id } => StructureChange::RemoveValue {
				id: parse_occurrence(id)?,
			},
			Self::RenameValue { id, name } => StructureChange::RenameValue {
				id: parse_occurrence(id)?,
				name,
			},
			Self::SetValueType {
				id,
				value_kind,
				value,
			} => StructureChange::SetValueType {
				id: parse_occurrence(id)?,
				kind: value_kind.value_kind(),
				value,
			},
		})
	}
}
