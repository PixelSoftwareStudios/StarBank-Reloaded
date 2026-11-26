use std::{collections::BTreeMap, iter, ops::Range, sync::Arc};

use sc2bank::{BankDocument, TypedAttribute, ValueKind, parse};
use starbank_core::Revision;

use crate::{
	EditorError, OccurrenceId, Session,
	identity::{Identities, Occurrence},
};

/// The projected differences between the opening snapshot and the current bank.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OpeningDiff {
	/// Whether any source bytes changed, including formatting, metadata, or a signature.
	pub source_changed: bool,
	/// Opening occurrences in opening order, followed by new occurrences in ID order.
	pub changes: Vec<OccurrenceDiff>,
}

/// The kind of a projected XML occurrence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OccurrenceKind {
	/// A bank section.
	Section,
	/// A section's key.
	Key,
	/// A named value child within a key.
	Value,
}

/// A difference addressed by an editor occurrence ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OccurrenceDiff {
	/// The identity from the opening or current projection.
	pub id: OccurrenceId,
	/// The level of the occurrence in the bank.
	pub kind: OccurrenceKind,
	/// The difference from the opening snapshot.
	pub change: OccurrenceChange,
}

/// A change to one projected occurrence or its source subtree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OccurrenceChange {
	/// The occurrence exists only in the current document.
	Added(DiffOccurrence),
	/// The occurrence exists only in the opening snapshot.
	Removed(DiffOccurrence),
	/// The matched occurrence or its subtree has different source bytes.
	Changed {
		/// The opening projection.
		before: DiffOccurrence,
		/// The current projection.
		after: DiffOccurrence,
	},
}

/// The identifying fields and optional typed attribute of an occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffOccurrence {
	/// The containing section or key in the same projection.
	pub parent: Option<OccurrenceId>,
	/// The decoded section/key name or the value's element name.
	pub name: Option<String>,
	/// The exact decoded lexeme, if the value has an unambiguous type.
	pub value: Option<DiffValue>,
}

/// The original or current lexical contents of a typed attribute.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffValue {
	/// The parsed type attribute name.
	pub kind: ValueKind,
	/// The decoded attribute text, including significant decimal digits.
	pub text: String,
}

impl Session {
	/// Compares the current bank to its immutable opening snapshot.
	///
	/// Section and key rows reflect changes in their descendants. A raw replacement creates new
	/// identities, so its projected occurrences appear as removals and additions. A byte-identical
	/// bank has no diff, even if its identities changed during the session.
	///
	/// # Errors
	///
	/// Returns an error if the opening snapshot cannot be parsed or an identity is inconsistent.
	pub fn opening_diff(&self) -> Result<OpeningDiff, EditorError> {
		if self.opening.as_ref() == self.document.source() {
			return Ok(OpeningDiff::default());
		}

		let opening = parse(&self.opening, self.limits).map_err(EditorError::Parse)?;
		let mut current = BTreeMap::new();
		for node in nodes(&self.document) {
			current.insert(node.id(&self.ids)?, node);
		}

		let mut changes = Vec::new();
		for before in nodes(&opening) {
			let id = before.id(&self.opening_ids)?;
			let change = match current.remove(&id) {
				Some(after) => {
					if before.source(&opening)? == after.source(&self.document)? {
						continue;
					}
					OccurrenceChange::Changed {
						before: before.snapshot(&self.opening_ids)?,
						after: after.snapshot(&self.ids)?,
					}
				}
				None => OccurrenceChange::Removed(before.snapshot(&self.opening_ids)?),
			};
			changes.push(OccurrenceDiff {
				id,
				kind: before.kind(),
				change,
			});
		}

		for (id, after) in current {
			changes.push(OccurrenceDiff {
				id,
				kind: after.kind(),
				change: OccurrenceChange::Added(after.snapshot(&self.ids)?),
			});
		}
		Ok(OpeningDiff {
			source_changed: true,
			changes,
		})
	}

	/// Restores one current value from the matching opening occurrence in one undo step.
	///
	/// # Errors
	///
	/// Rejects stale revisions, unmatched occurrences, and candidates that fail validation.
	pub fn revert_value(
		&mut self,
		expected: Revision,
		id: OccurrenceId,
	) -> Result<Revision, EditorError> {
		self.revert_occurrence(expected, id, OccurrenceKind::Value)
	}

	/// Restores one current key and its exact opening subtree in one undo step.
	///
	/// # Errors
	///
	/// Rejects stale revisions, unmatched occurrences, and candidates that fail validation.
	pub fn revert_key(
		&mut self,
		expected: Revision,
		id: OccurrenceId,
	) -> Result<Revision, EditorError> {
		self.revert_occurrence(expected, id, OccurrenceKind::Key)
	}

	/// Restores one current section and its exact opening subtree in one undo step.
	///
	/// # Errors
	///
	/// Rejects stale revisions, unmatched occurrences, and candidates that fail validation.
	pub fn revert_section(
		&mut self,
		expected: Revision,
		id: OccurrenceId,
	) -> Result<Revision, EditorError> {
		self.revert_occurrence(expected, id, OccurrenceKind::Section)
	}

	fn revert_occurrence(
		&mut self,
		expected: Revision,
		id: OccurrenceId,
		kind: OccurrenceKind,
	) -> Result<Revision, EditorError> {
		self.require_revision(expected)?;
		let Some(current_id) = self
			.ids
			.resolve(id)
			.filter(|&target| occurrence_kind(target) == kind)
		else {
			return Err(EditorError::RevertUnavailable(id));
		};
		let Some(original_id) = self.opening_ids.resolve(id) else {
			return Err(EditorError::RevertUnavailable(id));
		};
		let opening = parse(&self.opening, self.limits).map_err(EditorError::Parse)?;
		let target = nodes(&self.document)
			.find(|node| node.occurrence == current_id)
			.ok_or(EditorError::InconsistentProjection)?;
		let original = nodes(&opening)
			.find(|node| node.occurrence == original_id)
			.ok_or(EditorError::InconsistentProjection)?;
		let replacement = original.source(&opening)?;
		let source = self.document.source();
		let bytes = splice(
			source,
			&target.range,
			replacement,
			self.limits.document_bytes,
		)?;
		let candidate = parse(&bytes, self.limits).map_err(EditorError::Parse)?;
		let identities =
			self.revert_identities(&candidate, &opening, &target.range, &original.range)?;
		if bytes == source && identities == *self.ids {
			return Ok(self.revision());
		}
		self.commit(candidate, Arc::new(identities), self.id_cursor)
	}

	fn revert_identities(
		&self,
		candidate: &BankDocument,
		opening: &BankDocument,
		removed: &Range<usize>,
		restored: &Range<usize>,
	) -> Result<Identities, EditorError> {
		let mut positions = BTreeMap::new();
		for node in nodes(&self.document) {
			let mut start = node.range.start;
			if removed.contains(&start) {
				continue;
			}
			if start >= removed.end {
				start = start - removed.len() + restored.len();
			}
			positions.insert(start, (node.id(&self.ids)?, node.kind()));
		}
		for node in nodes(opening).filter(|node| restored.contains(&node.range.start)) {
			let start = removed.start + (node.range.start - restored.start);
			positions.insert(start, (node.id(&self.opening_ids)?, node.kind()));
		}

		let mut assignments = Vec::with_capacity(positions.len());
		for node in nodes(candidate) {
			let (id, _) = positions
				.remove(&node.range.start)
				.filter(|&(_, kind)| node.kind() == kind)
				.ok_or(EditorError::InconsistentProjection)?;
			assignments.push((id, node.occurrence));
		}
		if !positions.is_empty() {
			return Err(EditorError::InconsistentProjection);
		}
		Identities::from_assignments(assignments)
	}
}

fn splice(
	source: &[u8],
	target: &Range<usize>,
	replacement: &[u8],
	maximum: usize,
) -> Result<Vec<u8>, EditorError> {
	let before = source
		.get(..target.start)
		.ok_or(EditorError::InconsistentProjection)?;
	let after = source
		.get(target.end..)
		.ok_or(EditorError::InconsistentProjection)?;
	let length = before
		.len()
		.checked_add(after.len())
		.and_then(|length| length.checked_add(replacement.len()))
		.filter(|&length| length <= maximum)
		.ok_or(EditorError::EditLimit)?;
	let mut output = Vec::with_capacity(length);
	output.extend_from_slice(before);
	output.extend_from_slice(replacement);
	output.extend_from_slice(after);
	Ok(output)
}

struct Node<'bank> {
	occurrence: Occurrence,
	parent: Option<Occurrence>,
	name: Option<&'bank str>,
	attribute: Option<&'bank TypedAttribute>,
	range: Range<usize>,
}

impl Node<'_> {
	fn kind(&self) -> OccurrenceKind {
		occurrence_kind(self.occurrence)
	}

	fn id(&self, ids: &Identities) -> Result<OccurrenceId, EditorError> {
		ids.id(self.occurrence)
			.ok_or(EditorError::InconsistentProjection)
	}

	fn source<'bank>(&self, bank: &'bank BankDocument) -> Result<&'bank [u8], EditorError> {
		bank.source()
			.get(self.range.clone())
			.ok_or(EditorError::InconsistentProjection)
	}

	fn snapshot(&self, ids: &Identities) -> Result<DiffOccurrence, EditorError> {
		let parent = self
			.parent
			.map(|parent| ids.id(parent).ok_or(EditorError::InconsistentProjection))
			.transpose()?;
		Ok(DiffOccurrence {
			parent,
			name: self.name.map(str::to_owned),
			value: self.attribute.map(|attribute| DiffValue {
				kind: attribute.kind().clone(),
				text: attribute.value().to_owned(),
			}),
		})
	}
}

fn occurrence_kind(occurrence: Occurrence) -> OccurrenceKind {
	match occurrence {
		Occurrence::Section(_) => OccurrenceKind::Section,
		Occurrence::Key(_) => OccurrenceKind::Key,
		Occurrence::Value(_) => OccurrenceKind::Value,
	}
}

fn nodes(bank: &BankDocument) -> impl Iterator<Item = Node<'_>> {
	bank.sections().iter().flat_map(|section| {
		let section_id = Occurrence::Section(section.id());
		let section_node = Node {
			occurrence: section_id,
			parent: None,
			name: section.name(),
			attribute: None,
			range: section.source_range(),
		};
		iter::once(section_node).chain(section.keys().iter().flat_map(move |key| {
			let key_id = Occurrence::Key(key.id());
			let key_node = Node {
				occurrence: key_id,
				parent: Some(section_id),
				name: key.name(),
				attribute: None,
				range: key.source_range(),
			};
			iter::once(key_node).chain(key.values().iter().map(move |value| Node {
				occurrence: Occurrence::Value(value.id()),
				parent: Some(key_id),
				name: Some(value.name()),
				attribute: value.attribute(),
				range: value.source_range(),
			}))
		}))
	})
}
