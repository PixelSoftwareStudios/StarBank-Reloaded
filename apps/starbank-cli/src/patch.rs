use std::{collections::BTreeSet, path::Path};

use sc2bank::{Key, Section, ValueKind};
use serde::Deserialize;
use starbank_editor::{OccurrenceId, Session, StructureChange, ValueChange};

use crate::{
	bank::SHA256_BYTES,
	failure::{ErrorCode, Failure, Status},
	input,
};

const PATCH_SCHEMA_VERSION: u32 = 1;
const MAX_PATCH_BYTES: usize = 1024 * 1024;
const MAX_PATCH_OPERATIONS: usize = 4_096;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
	schema_version: u32,
	expected_sha256: String,
	operations: Vec<PatchOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum PatchOperation {
	SetValue {
		section: usize,
		key: usize,
		value: usize,
		lexeme: String,
	},
	AddSection {
		name: String,
	},
	RemoveSection {
		section: usize,
	},
	RenameSection {
		section: usize,
		name: String,
	},
	AddKey {
		section: usize,
		name: String,
	},
	RemoveKey {
		section: usize,
		key: usize,
	},
	RenameKey {
		section: usize,
		key: usize,
		name: String,
	},
	AddValue {
		section: usize,
		key: usize,
		name: String,
		kind: Kind,
		lexeme: String,
	},
	RemoveValue {
		section: usize,
		key: usize,
		value: usize,
	},
	RenameValue {
		section: usize,
		key: usize,
		value: usize,
		name: String,
	},
	SetValueType {
		section: usize,
		key: usize,
		value: usize,
		kind: Kind,
		lexeme: String,
	},
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
	Fixed,
	Flag,
	Int,
	Point,
	String,
	Text,
	Unit,
}

impl Kind {
	fn xml_type(self) -> &'static ValueKind {
		match self {
			Self::Fixed => &ValueKind::Fixed,
			Self::Flag => &ValueKind::Flag,
			Self::Int => &ValueKind::Int,
			Self::Point => &ValueKind::Point,
			Self::String => &ValueKind::String,
			Self::Text => &ValueKind::Text,
			Self::Unit => &ValueKind::Unit,
		}
	}
}

enum ResolvedOp<'input> {
	Value(ValueChange<'input>),
	Structure(StructureChange<'input>),
}

impl Patch {
	pub(crate) fn read_for_bank(
		path: &Path,
		observed_sha256: &[u8; SHA256_BYTES],
	) -> Result<Self, Failure> {
		let file = input::read(path, MAX_PATCH_BYTES)?;
		let patch: Self = serde_json::from_slice(file.bytes())
			.map_err(|_| invalid("The patch does not match the versioned JSON schema."))?;
		if patch.schema_version != PATCH_SCHEMA_VERSION {
			return Err(Failure::new(
				Status::Unsupported,
				ErrorCode::UnsupportedPatchVersion,
				"The patch schema version is unsupported.",
			));
		}
		if patch.operations.is_empty() || patch.operations.len() > MAX_PATCH_OPERATIONS {
			return Err(invalid("The patch must contain between 1 and 4,096 edits."));
		}
		let mut expected_sha256 = [0; SHA256_BYTES];
		hex::decode_to_slice(&patch.expected_sha256, &mut expected_sha256).map_err(|_| {
			invalid("The patch requires a 64-character hexadecimal SHA-256 digest.")
		})?;
		if observed_sha256 != &expected_sha256 {
			return Err(Failure::new(
				Status::Conflict,
				ErrorCode::InputChanged,
				"The bank bytes no longer match the patch fingerprint.",
			));
		}
		Ok(patch)
	}

	pub(crate) fn apply(&self, session: &mut Session) -> Result<(), Failure> {
		// Resolve every position in the fingerprinted input before structural changes move it.
		let operations = self
			.operations
			.iter()
			.map(|operation| operation.resolve(session))
			.collect::<Result<Vec<_>, _>>()?;
		let mut touched_values = BTreeSet::new();
		let mut batched_values = Vec::new();
		for operation in operations {
			match operation {
				ResolvedOp::Value(edit) => {
					if !touched_values.insert(edit.id) {
						return Err(invalid("A value occurs more than once in the patch."));
					}
					batched_values.push(edit);
				}
				ResolvedOp::Structure(change) => {
					flush_values(session, &mut batched_values)?;
					let _ = session.apply_structure(session.revision(), change)?;
				}
			}
		}
		flush_values(session, &mut batched_values)
	}
}

impl PatchOperation {
	fn resolve(&self, session: &Session) -> Result<ResolvedOp<'_>, Failure> {
		let locator = Locator { session };
		let change = match self {
			Self::SetValue {
				section,
				key,
				value,
				lexeme,
			} => {
				return Ok(ResolvedOp::Value(ValueChange {
					id: locator.value(*section, *key, *value)?,
					value: lexeme,
				}));
			}
			Self::AddSection { name } => StructureChange::AddSection { name },
			Self::RemoveSection { section } => StructureChange::RemoveSection {
				id: locator.section(*section)?,
			},
			Self::RenameSection { section, name } => StructureChange::RenameSection {
				id: locator.section(*section)?,
				name,
			},
			Self::AddKey { section, name } => StructureChange::AddKey {
				section: locator.section(*section)?,
				name,
			},
			Self::RemoveKey { section, key } => StructureChange::RemoveKey {
				id: locator.key(*section, *key)?,
			},
			Self::RenameKey { section, key, name } => StructureChange::RenameKey {
				id: locator.key(*section, *key)?,
				name,
			},
			Self::AddValue {
				section,
				key,
				name,
				kind,
				lexeme,
			} => StructureChange::AddValue {
				key: locator.key(*section, *key)?,
				name,
				kind: kind.xml_type(),
				value: lexeme,
			},
			Self::RemoveValue {
				section,
				key,
				value,
			} => StructureChange::RemoveValue {
				id: locator.value(*section, *key, *value)?,
			},
			Self::RenameValue {
				section,
				key,
				value,
				name,
			} => StructureChange::RenameValue {
				id: locator.value(*section, *key, *value)?,
				name,
			},
			Self::SetValueType {
				section,
				key,
				value,
				kind,
				lexeme,
			} => StructureChange::SetValueType {
				id: locator.value(*section, *key, *value)?,
				kind: kind.xml_type(),
				value: lexeme,
			},
		};
		Ok(ResolvedOp::Structure(change))
	}
}

struct Locator<'session> {
	session: &'session Session,
}

impl Locator<'_> {
	fn section_source(&self, section: usize) -> Option<&Section> {
		self.session.document().sections().get(section)
	}

	fn key_source(&self, section: usize, key: usize) -> Option<&Key> {
		self.section_source(section)?.keys().get(key)
	}

	fn section(&self, section: usize) -> Result<OccurrenceId, Failure> {
		self.section_source(section)
			.and_then(|section| self.session.id_of_section(section.id()))
			.ok_or_else(missing)
	}

	fn key(&self, section: usize, key: usize) -> Result<OccurrenceId, Failure> {
		self.key_source(section, key)
			.and_then(|key| self.session.id_of_key(key.id()))
			.ok_or_else(missing)
	}

	fn value(&self, section: usize, key: usize, value: usize) -> Result<OccurrenceId, Failure> {
		self.key_source(section, key)
			.and_then(|key| key.values().get(value))
			.and_then(|value| self.session.id_of_value(value.id()))
			.ok_or_else(missing)
	}
}

fn flush_values(session: &mut Session, edits: &mut Vec<ValueChange<'_>>) -> Result<(), Failure> {
	if !edits.is_empty() {
		let _ = session.set_values(session.revision(), edits)?;
		edits.clear();
	}
	Ok(())
}

fn missing() -> Failure {
	invalid("A patch occurrence was not found in the bank.")
}

fn invalid(message: &'static str) -> Failure {
	Failure::new(Status::Validation, ErrorCode::InvalidPatch, message)
}
