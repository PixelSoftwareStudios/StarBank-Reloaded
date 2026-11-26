use std::{collections::BTreeMap, ops::Range};

use crate::{BankDocument, KeyId, SectionId, ValueId, XmlEncoding, encoding::UTF16_UNIT_BYTES};

use super::{StructuralEditError, source::Patch};

/// Typed identity of a projected bank element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructuralNodeId {
	/// A section occurrence.
	Section(SectionId),
	/// A key occurrence.
	Key(KeyId),
	/// A value occurrence.
	Value(ValueId),
}

/// Correspondence between surviving element IDs before and after a structural edit.
#[derive(Clone, Debug, Default)]
pub struct StructuralIdMap {
	sections: BTreeMap<SectionId, SectionId>,
	keys: BTreeMap<KeyId, KeyId>,
	values: BTreeMap<ValueId, ValueId>,
}

impl StructuralIdMap {
	/// Returns the new ID for a surviving section.
	pub fn section(&self, old: SectionId) -> Option<SectionId> {
		self.sections.get(&old).copied()
	}

	/// Returns the new ID for a surviving key.
	pub fn key(&self, old: KeyId) -> Option<KeyId> {
		self.keys.get(&old).copied()
	}

	/// Returns the new ID for a surviving value.
	pub fn value(&self, old: ValueId) -> Option<ValueId> {
		self.values.get(&old).copied()
	}

	pub(super) fn reconcile(
		original: &BankDocument,
		current: &BankDocument,
		patches: &[Patch],
		inserted: Option<InsertedAt>,
	) -> Result<(Self, Option<StructuralNodeId>), StructuralEditError> {
		let shifts = native_shifts(original, patches)?;
		let inserted = inserted
			.map(|position| position.native(current))
			.transpose()?;
		let (sections, new_section) = correlate(
			original
				.sections()
				.iter()
				.map(|node| (node.source_range().start, node.id())),
			current
				.sections()
				.iter()
				.map(|node| (node.source_range().start, node.id())),
			&shifts,
			match inserted {
				Some(InsertedAt::Section(byte)) => Some(byte),
				_ => None,
			},
		)?;
		let (keys, new_key) = correlate(
			key_positions(original),
			key_positions(current),
			&shifts,
			match inserted {
				Some(InsertedAt::Key(byte)) => Some(byte),
				_ => None,
			},
		)?;
		let (values, new_value) = correlate(
			value_positions(original),
			value_positions(current),
			&shifts,
			match inserted {
				Some(InsertedAt::Value(byte)) => Some(byte),
				_ => None,
			},
		)?;
		let added = new_section
			.map(StructuralNodeId::Section)
			.or_else(|| new_key.map(StructuralNodeId::Key))
			.or_else(|| new_value.map(StructuralNodeId::Value));
		Ok((
			Self {
				sections,
				keys,
				values,
			},
			added,
		))
	}
}

#[derive(Clone, Copy)]
pub(super) enum InsertedAt {
	Section(usize),
	Key(usize),
	Value(usize),
}

impl InsertedAt {
	fn native(self, document: &BankDocument) -> Result<Self, StructuralEditError> {
		let (offset, wrap): (usize, fn(usize) -> Self) = match self {
			Self::Section(offset) => (offset, Self::Section),
			Self::Key(offset) => (offset, Self::Key),
			Self::Value(offset) => (offset, Self::Value),
		};
		let range = document
			.native_range(offset..offset)
			.ok_or(StructuralEditError::InvalidSourceSpan)?;
		Ok(wrap(range.start))
	}
}

struct NativeShift {
	range: Range<usize>,
	replacement_bytes: usize,
}

fn native_shifts(
	document: &BankDocument,
	patches: &[Patch],
) -> Result<Vec<NativeShift>, StructuralEditError> {
	patches
		.iter()
		.map(|patch| {
			let range = document
				.native_range(patch.range.clone())
				.ok_or(StructuralEditError::InvalidSourceSpan)?;
			let replacement_bytes = if document.encoding() == XmlEncoding::Utf8 {
				patch.replacement.len()
			} else {
				patch
					.replacement
					.encode_utf16()
					.count()
					.checked_mul(UTF16_UNIT_BYTES)
					.ok_or(StructuralEditError::UnexpectedProjection)?
			};
			Ok(NativeShift {
				range,
				replacement_bytes,
			})
		})
		.collect()
}

fn key_positions(document: &BankDocument) -> impl Iterator<Item = (usize, KeyId)> {
	document
		.sections()
		.iter()
		.flat_map(|section| section.keys())
		.map(|node| (node.source_range().start, node.id()))
}

fn value_positions(document: &BankDocument) -> impl Iterator<Item = (usize, ValueId)> {
	document
		.sections()
		.iter()
		.flat_map(|section| section.keys())
		.flat_map(|key| key.values())
		.map(|node| (node.source_range().start, node.id()))
}

fn correlate<Id: Ord>(
	original: impl Iterator<Item = (usize, Id)>,
	current: impl Iterator<Item = (usize, Id)>,
	patches: &[NativeShift],
	inserted: Option<usize>,
) -> Result<(BTreeMap<Id, Id>, Option<Id>), StructuralEditError> {
	let mut positions: BTreeMap<_, _> = current.collect();
	let mut remapped = BTreeMap::new();
	for (byte, previous) in original {
		let Some(byte) = shifted_byte(byte, patches)? else {
			continue;
		};
		let id = positions
			.remove(&byte)
			.ok_or(StructuralEditError::UnexpectedProjection)?;
		remapped.insert(previous, id);
	}

	let added = inserted
		.map(|byte| {
			positions
				.remove(&byte)
				.ok_or(StructuralEditError::UnexpectedProjection)
		})
		.transpose()?;
	if !positions.is_empty() {
		return Err(StructuralEditError::UnexpectedProjection);
	}
	Ok((remapped, added))
}

fn shifted_byte(
	byte: usize,
	patches: &[NativeShift],
) -> Result<Option<usize>, StructuralEditError> {
	let mut current = byte;
	for patch in patches {
		if byte < patch.range.start {
			break;
		}
		if patch.range.contains(&byte) {
			return Ok(None);
		}
		current = current
			.checked_sub(patch.range.len())
			.and_then(|reduced| reduced.checked_add(patch.replacement_bytes))
			.ok_or(StructuralEditError::UnexpectedProjection)?;
	}
	Ok(Some(current))
}
