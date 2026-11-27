use std::{
	borrow::Cow,
	collections::{BTreeMap, BTreeSet},
};

use sc2bank::{ValueEdit, ValueId, ValueNode};
use starbank_core::Revision;

use crate::{
	EditorError, LexemeError, OccurrenceId, Session, identity::Occurrence, normalize_value,
};

/// A proposed exact value replacement in an atomic transaction.
#[derive(Clone, Copy, Debug)]
pub struct ValueChange<'value> {
	/// Identifier from the current document projection.
	pub id: OccurrenceId,
	/// XML-decoded text, possibly including validated numeric grouping.
	pub value: &'value str,
}

/// A structured draft issue tied to a value occurrence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueIssue {
	/// Identifier supplied for the draft.
	pub id: OccurrenceId,
	/// The reason the draft is invalid.
	pub code: ValueIssueCode,
}

/// Why one occurrence's structured draft was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueIssueCode {
	/// The ID does not identify a current value.
	UnknownValue,
	/// The ID appeared earlier in the request.
	DuplicateValue,
	/// The value can only be edited in Raw View.
	OpaqueValue,
	/// The draft exceeds the document's scalar byte limit.
	ValueTooLarge,
	/// The integer grammar rejected the draft.
	InvalidInteger,
	/// The exact decimal grammar rejected the draft.
	InvalidFixed,
	/// The type does not support structured editing.
	UnsupportedKind,
}

impl ValueIssueCode {
	fn edit_error(self, id: OccurrenceId) -> EditorError {
		match self {
			Self::UnknownValue => EditorError::UnknownValue(id),
			Self::DuplicateValue => EditorError::DuplicateValue(id),
			Self::OpaqueValue => EditorError::OpaqueValue(id),
			Self::ValueTooLarge => EditorError::EditLimit,
			Self::InvalidInteger => EditorError::InvalidValue {
				id,
				source: LexemeError::InvalidInteger,
			},
			Self::InvalidFixed => EditorError::InvalidValue {
				id,
				source: LexemeError::InvalidFixed,
			},
			Self::UnsupportedKind => EditorError::InvalidValue {
				id,
				source: LexemeError::UnsupportedKind,
			},
		}
	}
}

impl Session {
	/// Checks draft occurrence IDs and value lexemes without changing the session.
	///
	/// Issues follow input order. Later occurrences of the same ID report duplicates.
	/// The commit still checks the complete serialized XML and aggregate document limits.
	///
	/// # Errors
	///
	/// Rejects stale revisions or batches above the document's node limit.
	pub fn validate_values(
		&self,
		expected: Revision,
		changes: &[ValueChange<'_>],
	) -> Result<Vec<ValueIssue>, EditorError> {
		self.require_value_batch(expected, changes)?;
		let mut seen = BTreeSet::new();
		let mut requested = BTreeMap::new();
		let mut codes = vec![None; changes.len()];
		for (index, change) in changes.iter().enumerate() {
			if !seen.insert(change.id) {
				codes[index] = Some(ValueIssueCode::DuplicateValue);
				continue;
			}
			match self.requested_value(change) {
				Ok(id) => {
					requested.insert(id, index);
				}
				Err(code) => codes[index] = Some(code),
			}
		}
		let mut nodes = self.value_nodes();
		while !requested.is_empty()
			&& let Some(value) = nodes.next()
		{
			if let Some(index) = requested.remove(&value.id()) {
				codes[index] = normalize_change(value, changes[index].value).err();
			}
		}
		for index in requested.into_values() {
			codes[index] = Some(ValueIssueCode::UnknownValue);
		}
		Ok(changes
			.iter()
			.zip(codes)
			.filter_map(|(change, code)| {
				code.map(|code| ValueIssue {
					id: change.id,
					code,
				})
			})
			.collect())
	}

	/// Applies all checked changes in one revision and one undo step.
	///
	/// # Errors
	///
	/// Rejects stale revisions, invalid values, repeated targets and limit violations.
	pub fn set_values(
		&mut self,
		expected: Revision,
		changes: &[ValueChange<'_>],
	) -> Result<Revision, EditorError> {
		self.require_value_batch(expected, changes)?;
		let mut requested = BTreeMap::new();
		for change in changes {
			let value = self
				.requested_value(change)
				.map_err(|code| code.edit_error(change.id))?;
			if requested.insert(value, change).is_some() {
				return Err(EditorError::DuplicateValue(change.id));
			}
		}
		if requested.is_empty() {
			return Ok(self.revision());
		}
		let mut normalized = Vec::with_capacity(requested.len());
		for value in self.value_nodes() {
			let Some(change) = requested.remove(&value.id()) else {
				continue;
			};
			let lexeme =
				normalize_change(value, change.value).map_err(|code| code.edit_error(change.id))?;
			if let Some(lexeme) = lexeme {
				normalized.push((value.id(), lexeme));
			}
			if requested.is_empty() {
				break;
			}
		}
		if let Some((_, change)) = requested.first_key_value() {
			return Err(EditorError::UnknownValue(change.id));
		}
		let edits: Vec<_> = normalized
			.iter()
			.map(|(id, text)| ValueEdit {
				id: *id,
				value: text.as_ref(),
			})
			.collect();
		self.commit_value_edits(&edits)
	}

	fn require_value_batch(
		&self,
		expected: Revision,
		changes: &[ValueChange<'_>],
	) -> Result<(), EditorError> {
		self.require_revision(expected)?;
		if changes.len() > usize::try_from(self.limits.nodes).unwrap_or(usize::MAX) {
			return Err(EditorError::EditLimit);
		}
		Ok(())
	}

	fn requested_value(&self, change: &ValueChange<'_>) -> Result<ValueId, ValueIssueCode> {
		if change.value.len() > self.limits.scalar_bytes {
			return Err(ValueIssueCode::ValueTooLarge);
		}
		match self.ids.resolve(change.id) {
			Some(Occurrence::Value(id)) => Ok(id),
			_ => Err(ValueIssueCode::UnknownValue),
		}
	}

	fn value_nodes(&self) -> impl Iterator<Item = &ValueNode> {
		self.document
			.sections()
			.iter()
			.flat_map(|section| section.keys())
			.flat_map(|key| key.values())
	}
}

fn normalize_change<'value>(
	node: &ValueNode,
	proposed: &'value str,
) -> Result<Option<Cow<'value, str>>, ValueIssueCode> {
	let attribute = node.attribute().ok_or(ValueIssueCode::OpaqueValue)?;
	if proposed == attribute.value() {
		return Ok(None);
	}
	normalize_value(attribute.kind(), proposed)
		.map(Some)
		.map_err(|error| match error {
			LexemeError::InvalidInteger => ValueIssueCode::InvalidInteger,
			LexemeError::InvalidFixed => ValueIssueCode::InvalidFixed,
			LexemeError::UnsupportedKind => ValueIssueCode::UnsupportedKind,
		})
}
