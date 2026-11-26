use std::{fmt, mem, num::NonZeroU64, str::FromStr};

use sc2bank::{BankDocument, KeyId, SectionId, ValueId};

use crate::EditorError;

/// Stable session identifier for one XML occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OccurrenceId(NonZeroU64);

impl fmt::Display for OccurrenceId {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(formatter)
	}
}

impl FromStr for OccurrenceId {
	type Err = ParseOccurrenceIdError;

	fn from_str(text: &str) -> Result<Self, Self::Err> {
		if text.starts_with('0') || !text.bytes().all(|byte| byte.is_ascii_digit()) {
			return Err(ParseOccurrenceIdError);
		}
		text.parse::<NonZeroU64>()
			.map(Self)
			.map_err(|_| ParseOccurrenceIdError)
	}
}

/// The string does not contain a canonical nonzero occurrence identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseOccurrenceIdError;

impl fmt::Display for ParseOccurrenceIdError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str("occurrence ID must be a canonical nonzero decimal integer")
	}
}

impl std::error::Error for ParseOccurrenceIdError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Occurrence {
	Section(SectionId),
	Key(KeyId),
	Value(ValueId),
}

impl Occurrence {
	fn source_index(self) -> u32 {
		match self {
			Self::Section(id) => id.get(),
			Self::Key(id) => id.get(),
			Self::Value(id) => id.get(),
		}
	}
}

#[derive(Debug, Default)]
pub(crate) struct Identities {
	by_source: Vec<(u32, OccurrenceId)>,
	by_session: Vec<(OccurrenceId, Occurrence)>,
}

impl Identities {
	pub(crate) fn allocate(document: &BankDocument, cursor: &mut u64) -> Result<Self, EditorError> {
		let mut identities = Self::default();
		for section in document.sections() {
			identities.insert(Occurrence::Section(section.id()), cursor)?;
			for key in section.keys() {
				identities.insert(Occurrence::Key(key.id()), cursor)?;
				for value in key.values() {
					identities.insert(Occurrence::Value(value.id()), cursor)?;
				}
			}
		}
		identities
			.by_source
			.sort_unstable_by_key(|&(index, _)| index);
		Ok(identities)
	}

	fn insert(&mut self, occurrence: Occurrence, cursor: &mut u64) -> Result<(), EditorError> {
		let next = cursor
			.checked_add(1)
			.and_then(NonZeroU64::new)
			.ok_or(EditorError::IdentityExhausted)?;
		*cursor = next.get();
		let id = OccurrenceId(next);
		self.by_source.push((occurrence.source_index(), id));
		self.by_session.push((id, occurrence));
		Ok(())
	}

	pub(crate) fn id(&self, occurrence: Occurrence) -> Option<OccurrenceId> {
		let index = self
			.by_source
			.binary_search_by_key(&occurrence.source_index(), |&(index, _)| index)
			.ok()?;
		Some(self.by_source[index].1)
	}

	pub(crate) fn resolve(&self, id: OccurrenceId) -> Option<Occurrence> {
		let index = self
			.by_session
			.binary_search_by_key(&id, |&(identifier, _)| identifier)
			.ok()?;
		Some(self.by_session[index].1)
	}

	pub(crate) fn allocation_bytes(&self) -> usize {
		mem::size_of_val(self.by_source.as_slice()) + mem::size_of_val(self.by_session.as_slice())
	}
}
