use std::{
	collections::{BTreeMap, BTreeSet},
	ops::Range,
	sync::Arc,
};

use sc2bank::BankDocument;
use starbank_core::Revision;

use crate::{
	EditorError, OccurrenceId, Session,
	identity::{Identities, Occurrence},
};

#[derive(Debug)]
pub(crate) struct Checkpoint {
	bytes: Arc<[u8]>,
	ids: Arc<Identities>,
	sections: BTreeMap<OccurrenceId, Range<usize>>,
}

impl Checkpoint {
	pub(crate) fn new(
		bytes: Arc<[u8]>,
		document: &BankDocument,
		ids: Arc<Identities>,
	) -> Result<Self, EditorError> {
		let sections = document
			.sections()
			.iter()
			.map(|section| {
				let id = ids
					.id(Occurrence::Section(section.id()))
					.ok_or(EditorError::InconsistentProjection)?;
				Ok((id, section.source_range()))
			})
			.collect::<Result<_, EditorError>>()?;
		Ok(Self {
			bytes,
			ids,
			sections,
		})
	}

	pub(crate) fn bytes(&self) -> &[u8] {
		&self.bytes
	}
}

/// Section changes since the last verified active save or, before a save, opening.
#[derive(Debug, Default)]
pub struct DirtySections {
	current: BTreeSet<OccurrenceId>,
	deleted: usize,
}

impl DirtySections {
	/// Whether a current section has unsaved changes anywhere in its source subtree.
	pub fn contains(&self, id: OccurrenceId) -> bool {
		self.current.contains(&id)
	}

	/// The number of added, changed, or deleted section occurrences.
	pub fn count(&self) -> usize {
		self.current.len() + self.deleted
	}
}

impl Session {
	/// Reports pending changes to section subtrees relative to the saved checkpoint.
	///
	/// Whole-document dirtiness also includes formatting and metadata outside sections.
	/// When a raw edit invalidates identities, byte-identical sections with new IDs
	/// are matched by complete source bytes. Structured edits within the same
	/// projection keep added and deleted occurrences distinct.
	///
	/// # Errors
	///
	/// Returns an error if the current projection has an inconsistent section ID.
	pub fn dirty_sections(&self) -> Result<DirtySections, EditorError> {
		if !self.is_dirty() {
			return Ok(DirtySections::default());
		}
		let saved = &self.saved;
		let current_source = self.document.source();
		let mut dirty = DirtySections::default();
		let mut matched_saved = 0;
		let mut new_sections = Vec::new();

		for section in self.document.sections() {
			let id = self
				.id_of_section(section.id())
				.ok_or(EditorError::InconsistentProjection)?;
			let current = &current_source[section.source_range()];
			if let Some(range) = saved.sections.get(&id) {
				matched_saved += 1;
				if current != &saved.bytes[range.clone()] {
					dirty.current.insert(id);
				}
			} else {
				new_sections.push((id, current));
			}
		}

		dirty.deleted = saved.sections.len() - matched_saved;
		if !self.ids.same_raw_origin(&saved.ids) && dirty.deleted > 0 && !new_sections.is_empty() {
			let mut unmatched_source = BTreeMap::new();
			for (&id, range) in &saved.sections {
				if self.ids.resolve(id).is_none() {
					*unmatched_source
						.entry(&saved.bytes[range.clone()])
						.or_insert(0) += 1;
				}
			}
			for (id, source) in new_sections {
				let matches = unmatched_source.get_mut(source).filter(|count| **count > 0);
				if let Some(matches) = matches {
					*matches -= 1;
					dirty.deleted -= 1;
				} else {
					dirty.current.insert(id);
				}
			}
		} else {
			dirty
				.current
				.extend(new_sections.into_iter().map(|(id, _)| id));
		}
		Ok(dirty)
	}

	/// Restores the exact last verified active save, or the opening state before a save.
	///
	/// This is a single undoable in-memory edit. It keeps the active signing context
	/// and does not change either checkpoint.
	///
	/// # Errors
	///
	/// Rejects a stale or exhausted revision.
	pub fn revert_to_saved(&mut self, expected: Revision) -> Result<Revision, EditorError> {
		self.restore_checkpoint(expected, self.saved.bytes.clone(), self.saved.ids.clone())
	}
}
