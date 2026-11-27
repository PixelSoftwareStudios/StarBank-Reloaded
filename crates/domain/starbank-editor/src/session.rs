use std::{collections::BTreeMap, sync::Arc};

use sc2bank::{BankDocument, KeyId, Limits, SectionId, ValueEdit, ValueId, parse, serialize};
use starbank_core::Revision;
use starbank_signature::{SignatureStatus, SigningContext, verify_signature};

use crate::{
	EditorError, OccurrenceId,
	checkpoint::Checkpoint,
	history::{Direction, History},
	identity::{Identities, Occurrence},
	normalize_value,
	prepare::SaveOrder,
	raw::validate_raw_numbers,
};

/// A proposed exact value replacement in an atomic transaction.
#[derive(Clone, Copy, Debug)]
pub struct ValueChange<'value> {
	/// Identifier from the current document projection.
	pub id: OccurrenceId,
	/// XML-decoded text, possibly including validated numeric grouping.
	pub value: &'value str,
}

/// Single owner of one open document and its revision, checkpoints and edit history.
#[derive(Debug)]
pub struct Session {
	pub(crate) document: BankDocument,
	pub(crate) limits: Limits,
	pub(crate) context: Option<SigningContext>,
	pub(crate) ids: Arc<Identities>,
	pub(crate) id_cursor: u64,
	pub(crate) opening: Arc<[u8]>,
	pub(crate) token: Arc<()>,
	pub(crate) save_order: SaveOrder,
	pub(crate) opening_ids: Arc<Identities>,
	pub(crate) saved: Checkpoint,
	revision: Revision,
	history: History,
}

impl Session {
	/// Opens a bank and records the exact opening and saved checkpoints.
	///
	/// # Errors
	///
	/// Rejects malformed XML, exceeded limits or an exhausted ID space.
	pub fn open(
		bytes: &[u8],
		limits: Limits,
		context: Option<SigningContext>,
	) -> Result<Self, EditorError> {
		let document = parse(bytes, limits).map_err(EditorError::Parse)?;
		let mut id_cursor = 0;
		let ids = Arc::new(Identities::allocate(&document, &mut id_cursor)?);
		let opening: Arc<[u8]> = bytes.into();
		let saved = Checkpoint::new(opening.clone(), &document, ids.clone())?;
		Ok(Self {
			document,
			limits,
			context,
			id_cursor,
			saved,
			opening,
			token: Arc::new(()),
			save_order: SaveOrder::default(),
			opening_ids: ids.clone(),
			ids,
			revision: Revision::INITIAL,
			history: History::default(),
		})
	}

	/// The last validated XML document in stored source order.
	pub fn document(&self) -> &BankDocument {
		&self.document
	}

	/// Monotonic version of this editing session.
	pub fn revision(&self) -> Revision {
		self.revision
	}

	/// Whether the checked XML differs from the verified saved checkpoint.
	pub fn is_dirty(&self) -> bool {
		self.document.source() != self.saved.bytes()
	}

	/// Whether the undo direction is available.
	pub fn can_undo(&self) -> bool {
		self.history.can_undo()
	}

	/// Whether the redo direction is available.
	pub fn can_redo(&self) -> bool {
		self.history.can_redo()
	}

	/// Verification status of the stored signature under the current context.
	pub fn signature_status(&self) -> SignatureStatus {
		verify_signature(&self.document, self.context.as_ref())
	}

	/// Looks up the editor ID for a section in the checked document.
	pub fn id_of_section(&self, id: SectionId) -> Option<OccurrenceId> {
		self.ids.id(Occurrence::Section(id))
	}

	/// Looks up the editor ID for a key in the checked document.
	pub fn id_of_key(&self, id: KeyId) -> Option<OccurrenceId> {
		self.ids.id(Occurrence::Key(id))
	}

	/// Looks up the editor ID for a value in the checked document.
	pub fn id_of_value(&self, id: ValueId) -> Option<OccurrenceId> {
		self.ids.id(Occurrence::Value(id))
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
		self.require_revision(expected)?;
		if changes.len() > usize::try_from(self.limits.nodes).unwrap_or(usize::MAX) {
			return Err(EditorError::EditLimit);
		}
		let mut requested = BTreeMap::new();
		for change in changes {
			if change.value.len() > self.limits.scalar_bytes {
				return Err(EditorError::EditLimit);
			}
			let Some(Occurrence::Value(value)) = self.ids.resolve(change.id) else {
				return Err(EditorError::UnknownValue(change.id));
			};
			if requested.insert(value, change).is_some() {
				return Err(EditorError::DuplicateValue(change.id));
			}
		}
		if requested.is_empty() {
			return Ok(self.revision);
		}
		let mut normalized = Vec::with_capacity(requested.len());
		for value in self
			.document
			.sections()
			.iter()
			.flat_map(|section| section.keys())
			.flat_map(|key| key.values())
		{
			let Some(change) = requested.remove(&value.id()) else {
				continue;
			};
			let attribute = value
				.attribute()
				.ok_or(EditorError::OpaqueValue(change.id))?;
			if change.value == attribute.value() {
				continue;
			}
			let lexeme = normalize_value(attribute.kind(), change.value).map_err(|source| {
				EditorError::InvalidValue {
					id: change.id,
					source,
				}
			})?;
			normalized.push((value.id(), lexeme));
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

	pub(crate) fn commit_value_edits(
		&mut self,
		edits: &[ValueEdit<'_>],
	) -> Result<Revision, EditorError> {
		if edits.is_empty() {
			return Ok(self.revision);
		}
		let bytes =
			serialize(&self.document, edits, self.limits).map_err(EditorError::Serialize)?;
		if bytes == self.document.source() {
			return Ok(self.revision);
		}
		let document = parse(&bytes, self.limits).map_err(EditorError::Parse)?;
		self.commit(document, self.ids.clone(), self.id_cursor)
	}

	/// Applies an exact raw replacement, invalidating prior occurrence identities.
	///
	/// Raw numeric attributes use ungrouped integer or decimal text. Grouped input can
	/// be entered through structured edits. Raw XML is retained byte-for-byte, and
	/// inherited malformed numeric lexemes can remain unchanged at their projected path.
	///
	/// # Errors
	///
	/// Rejects stale revisions, encoding changes, invalid XML, and new invalid
	/// or grouped numeric attributes. The previous checked document stays intact.
	pub fn replace_raw(
		&mut self,
		expected: Revision,
		bytes: &[u8],
	) -> Result<Revision, EditorError> {
		self.require_revision(expected)?;
		if bytes == self.document.source() {
			return Ok(self.revision);
		}
		let document = parse(bytes, self.limits).map_err(EditorError::Parse)?;
		if document.encoding() != self.document.encoding() {
			return Err(EditorError::RawEncodingMismatch);
		}
		validate_raw_numbers(&self.document, &document)?;
		let mut cursor = self.id_cursor;
		let identities = Arc::new(Identities::allocate(&document, &mut cursor)?);
		self.commit(document, identities, cursor)
	}

	/// Restores the immutable opening snapshot without a file operation.
	///
	/// # Errors
	///
	/// Rejects a stale or exhausted revision.
	pub fn reset(&mut self, expected: Revision) -> Result<Revision, EditorError> {
		self.restore_checkpoint(expected, self.opening.clone(), self.opening_ids.clone())
	}

	pub(crate) fn restore_checkpoint(
		&mut self,
		expected: Revision,
		bytes: Arc<[u8]>,
		ids: Arc<Identities>,
	) -> Result<Revision, EditorError> {
		self.require_revision(expected)?;
		if self.document.source() == bytes.as_ref() && self.ids == ids {
			return Ok(self.revision);
		}
		let document = parse(&bytes, self.limits).map_err(EditorError::Parse)?;
		self.commit(document, ids, self.id_cursor)
	}

	/// Reverses one transaction and advances the revision.
	///
	/// # Errors
	///
	/// Rejects invalid revisions or unavailable history.
	pub fn undo(&mut self, expected: Revision) -> Result<Revision, EditorError> {
		self.restore(expected, Direction::Undo)
	}

	/// Reapplies one undone transaction and advances the revision.
	///
	/// # Errors
	///
	/// Rejects invalid revisions or unavailable history.
	pub fn redo(&mut self, expected: Revision) -> Result<Revision, EditorError> {
		self.restore(expected, Direction::Redo)
	}

	fn restore(
		&mut self,
		expected: Revision,
		direction: Direction,
	) -> Result<Revision, EditorError> {
		self.require_revision(expected)?;
		let revision = self.next_revision()?;
		let (bytes, identities) = self.history.restore(self.document.source(), direction)?;
		let document = parse(&bytes, self.limits).map_err(EditorError::Parse)?;
		self.history.advance(direction);
		self.document = document;
		self.ids = identities;
		self.revision = revision;
		Ok(revision)
	}

	pub(crate) fn require_revision(&self, expected: Revision) -> Result<(), EditorError> {
		if expected != self.revision {
			return Err(EditorError::StaleRevision {
				expected,
				current: self.revision,
			});
		}
		Ok(())
	}

	pub(crate) fn next_revision(&self) -> Result<Revision, EditorError> {
		self.revision
			.checked_next()
			.ok_or(EditorError::RevisionExhausted)
	}

	pub(crate) fn commit(
		&mut self,
		document: BankDocument,
		identities: Arc<Identities>,
		cursor: u64,
	) -> Result<Revision, EditorError> {
		let revision = self.next_revision()?;
		self.history.record(
			self.document.source(),
			document.source(),
			&self.ids,
			&identities,
		);
		self.document = document;
		self.ids = identities;
		self.id_cursor = cursor;
		self.revision = revision;
		Ok(revision)
	}
}
