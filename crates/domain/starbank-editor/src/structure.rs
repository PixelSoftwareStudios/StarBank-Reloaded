use std::sync::Arc;

use sc2bank::{KeyId, SectionId, StructuralEdit, ValueId, ValueKind, apply_structural_edit};
use starbank_core::Revision;

use crate::{EditorError, OccurrenceId, Session, identity::Occurrence, normalize_value};

/// One structural change addressed by stable session identifiers.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum StructureChange<'input> {
	/// Add a section after the current sections.
	AddSection {
		/// Decoded section name.
		name: &'input str,
	},
	/// Remove a section and its contents.
	RemoveSection {
		/// Section occurrence from the current revision.
		id: OccurrenceId,
	},
	/// Replace a section's name, including when it is missing.
	RenameSection {
		/// Section occurrence from the current revision.
		id: OccurrenceId,
		/// Decoded section name.
		name: &'input str,
	},
	/// Append a key under a section.
	AddKey {
		/// Parent section occurrence from the current revision.
		section: OccurrenceId,
		/// Decoded key name.
		name: &'input str,
	},
	/// Remove a key and its values.
	RemoveKey {
		/// Key occurrence from the current revision.
		id: OccurrenceId,
	},
	/// Replace a key's name, including when it is missing.
	RenameKey {
		/// Key occurrence from the current revision.
		id: OccurrenceId,
		/// Decoded key name.
		name: &'input str,
	},
	/// Append one typed value to a key.
	AddValue {
		/// Parent key occurrence from the current revision.
		key: OccurrenceId,
		/// Unqualified XML element name.
		name: &'input str,
		/// Type of the new value.
		kind: &'input ValueKind,
		/// Value text to validate against the selected type.
		value: &'input str,
	},
	/// Remove a value element.
	RemoveValue {
		/// Value occurrence from the current revision.
		id: OccurrenceId,
	},
	/// Change a value element's name without changing its type or content.
	RenameValue {
		/// Value occurrence from the current revision.
		id: OccurrenceId,
		/// Unqualified XML element name.
		name: &'input str,
	},
	/// Replace a value's type and its lexical content together.
	SetValueType {
		/// Value occurrence from the current revision.
		id: OccurrenceId,
		/// Replacement type.
		kind: &'input ValueKind,
		/// Value text to validate against the replacement type.
		value: &'input str,
	},
}

/// Committed revision and optional newly allocated session identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StructuralOutcome {
	/// The current revision after the operation.
	pub revision: Revision,
	/// Stable identifier of the new occurrence for an addition.
	pub added: Option<OccurrenceId>,
}

impl Session {
	/// Applies a validated structural change as a single undoable transaction.
	///
	/// Surviving occurrences retain their identifiers. A no-op returns the current
	/// revision and leaves history intact. Newly added identifiers are allocated
	/// monotonically, including when an earlier addition was undone.
	///
	/// # Errors
	///
	/// Rejects stale revisions, wrong occurrence kinds, invalid values, unsupported
	/// source structures, exceeded limits, and exhausted revisions or identities.
	pub fn apply_structure(
		&mut self,
		expected: Revision,
		change: StructureChange<'_>,
	) -> Result<StructuralOutcome, EditorError> {
		self.require_revision(expected)?;
		let normalized;
		let edit = match change {
			StructureChange::AddSection { name } => StructuralEdit::AddSection { name },
			StructureChange::RemoveSection { id } => StructuralEdit::RemoveSection {
				id: self.section_id(id)?,
			},
			StructureChange::RenameSection { id, name } => StructuralEdit::RenameSection {
				id: self.section_id(id)?,
				name,
			},
			StructureChange::AddKey { section, name } => StructuralEdit::AddKey {
				section: self.section_id(section)?,
				name,
			},
			StructureChange::RemoveKey { id } => StructuralEdit::RemoveKey {
				id: self.key_id(id)?,
			},
			StructureChange::RenameKey { id, name } => StructuralEdit::RenameKey {
				id: self.key_id(id)?,
				name,
			},
			StructureChange::AddValue {
				key,
				name,
				kind,
				value,
			} => {
				let parent = self.key_id(key)?;
				self.require_structured_value_size(value)?;
				normalized = normalize_value(kind, value)
					.map_err(|source| EditorError::InvalidAddedValue { key, source })?;
				StructuralEdit::AddValue {
					key: parent,
					name,
					kind,
					value: &normalized,
				}
			}
			StructureChange::RemoveValue { id } => StructuralEdit::RemoveValue {
				id: self.value_id(id)?,
			},
			StructureChange::RenameValue { id, name } => StructuralEdit::RenameValue {
				id: self.value_id(id)?,
				name,
			},
			StructureChange::SetValueType { id, kind, value } => {
				let target = self.value_id(id)?;
				self.require_structured_value_size(value)?;
				normalized = normalize_value(kind, value)
					.map_err(|source| EditorError::InvalidValue { id, source })?;
				StructuralEdit::SetValueType {
					id: target,
					kind,
					value: &normalized,
				}
			}
		};

		let result = apply_structural_edit(&self.document, edit, self.limits)
			.map_err(EditorError::Structure)?;
		if result.document.source() == self.document.source() {
			return Ok(StructuralOutcome {
				revision: self.revision(),
				added: None,
			});
		}

		let mut cursor = self.id_cursor;
		let (identities, added) = self.ids.remap(&result.remap, result.added, &mut cursor)?;
		let revision = self.commit(result.document, Arc::new(identities), cursor)?;
		Ok(StructuralOutcome { revision, added })
	}

	fn section_id(&self, id: OccurrenceId) -> Result<SectionId, EditorError> {
		match self.ids.resolve(id) {
			Some(Occurrence::Section(source)) => Ok(source),
			_ => Err(EditorError::UnknownSection(id)),
		}
	}

	fn key_id(&self, id: OccurrenceId) -> Result<KeyId, EditorError> {
		match self.ids.resolve(id) {
			Some(Occurrence::Key(source)) => Ok(source),
			_ => Err(EditorError::UnknownKey(id)),
		}
	}

	fn value_id(&self, id: OccurrenceId) -> Result<ValueId, EditorError> {
		match self.ids.resolve(id) {
			Some(Occurrence::Value(source)) => Ok(source),
			_ => Err(EditorError::UnknownValue(id)),
		}
	}

	fn require_structured_value_size(&self, input: &str) -> Result<(), EditorError> {
		if input.len() > self.limits.scalar_bytes {
			return Err(EditorError::EditLimit);
		}
		Ok(())
	}
}
