use std::{
	borrow::Cow,
	collections::{BTreeMap, BTreeSet},
};

use sc2bank::{LimitKind, SerializeError, ValueEdit, ValueId, ValueNode, validate_value_scalar};
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
	/// The draft or its escaped XML exceeds the document's scalar byte limit.
	ValueTooLarge,
	/// The draft contains a character disallowed in XML.
	InvalidXmlCharacter,
	/// The integer grammar rejected the draft.
	InvalidInteger,
	/// The exact decimal grammar rejected the draft.
	InvalidFixed,
	/// The type does not support structured editing.
	UnsupportedKind,
}

fn issue_code(error: EditorError) -> Result<ValueIssueCode, EditorError> {
	Ok(match error {
		EditorError::UnknownValue(_) => ValueIssueCode::UnknownValue,
		EditorError::OpaqueValue(_) => ValueIssueCode::OpaqueValue,
		EditorError::EditLimit
		| EditorError::Serialize(SerializeError::LimitExceeded(LimitKind::ScalarBytes)) => {
			ValueIssueCode::ValueTooLarge
		}
		EditorError::Serialize(SerializeError::InvalidCharacter { .. }) => {
			ValueIssueCode::InvalidXmlCharacter
		}
		EditorError::InvalidValue { source, .. } => match source {
			LexemeError::InvalidInteger => ValueIssueCode::InvalidInteger,
			LexemeError::InvalidFixed => ValueIssueCode::InvalidFixed,
			LexemeError::UnsupportedKind => ValueIssueCode::UnsupportedKind,
		},
		other => return Err(other),
	})
}

impl Session {
	/// Checks draft occurrences, lexemes and XML scalars without changing the session.
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
				Err(error) => codes[index] = Some(issue_code(error)?),
			}
		}
		let mut nodes = self.value_nodes();
		while !requested.is_empty()
			&& let Some(value) = nodes.next()
		{
			if let Some(index) = requested.remove(&value.id()) {
				let check = normalize_change(value, &changes[index]).and_then(|lexeme| {
					if let Some(lexeme) = lexeme {
						validate_value_scalar(&self.document, value, &lexeme, self.limits)
							.map_err(EditorError::Serialize)?;
					}
					Ok(())
				});
				if let Err(error) = check {
					codes[index] = Some(issue_code(error)?);
				}
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
			let value = self.requested_value(change)?;
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
			let lexeme = normalize_change(value, change)?;
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

	fn requested_value(&self, change: &ValueChange<'_>) -> Result<ValueId, EditorError> {
		if change.value.len() > self.limits.scalar_bytes {
			return Err(EditorError::EditLimit);
		}
		match self.ids.resolve(change.id) {
			Some(Occurrence::Value(id)) => Ok(id),
			_ => Err(EditorError::UnknownValue(change.id)),
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
	change: &ValueChange<'value>,
) -> Result<Option<Cow<'value, str>>, EditorError> {
	let attribute = node
		.attribute()
		.ok_or(EditorError::OpaqueValue(change.id))?;
	if change.value == attribute.value() {
		return Ok(None);
	}
	normalize_value(attribute.kind(), change.value)
		.map(Some)
		.map_err(|source| EditorError::InvalidValue {
			id: change.id,
			source,
		})
}
