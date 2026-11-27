use std::{io::Cursor, ops::Range};

use stormlib_rs::{BlockTableEntry, HashTableEntry, MpqFileFlags, lookup_file_name};

use crate::{V1AppendError, V1LayoutIssue, archive::ClassicTables, header::Header};

const INTERNAL_NAMES: &[&str] = &[
	"(listfile)",
	"(attributes)",
	"(signature)",
	"(patch_metadata)",
];
const PACKED: MpqFileFlags = MpqFileFlags::COMPRESS.union(MpqFileFlags::IMPLODE);
const RETAINED_FLAGS: MpqFileFlags = MpqFileFlags::EXISTS
	.union(MpqFileFlags::SINGLE_UNIT)
	.union(MpqFileFlags::SECTOR_CRC)
	.union(PACKED);

pub(super) struct Layout {
	pub(super) tables: ClassicTables,
	pub(super) payload: Range<usize>,
	pub(super) sector_bytes: usize,
}

impl Layout {
	pub(super) fn read(bytes: &[u8]) -> Result<Self, V1AppendError> {
		let mut reader = Cursor::new(bytes);
		let header = Header::read(&mut reader)?;
		if !header.is_v1() {
			return Err(V1LayoutIssue::Version.into());
		}
		if header.block_table.end != bytes.len() as u64 {
			return Err(V1LayoutIssue::Trailer.into());
		}
		let tables = ClassicTables::read(&mut reader, &header)?;
		let names = NameGuard::new(&tables);
		if INTERNAL_NAMES.iter().any(|name| names.contains(name)) {
			return Err(V1LayoutIssue::InternalMember.into());
		}
		check_references(&tables)?;
		let payload = slice_range(header.payload);
		check_payload(&tables.block, &payload)?;
		Ok(Self {
			tables,
			payload,
			sector_bytes: header.sector_bytes,
		})
	}

	pub(super) fn name_guard(&self) -> NameGuard {
		NameGuard::new(&self.tables)
	}
}

/// Performs existence checks across all locales, even beyond a misplaced empty slot.
pub(super) struct NameGuard {
	hash: Vec<HashTableEntry>,
	block_count: usize,
}

impl NameGuard {
	fn new(tables: &ClassicTables) -> Self {
		let mut hash = tables.hash.clone();
		for slot in &mut hash {
			if slot.is_free() {
				// The shared lookup skips tombstones while keeping the same
				// published name hashing. Scan the full table for collision checks.
				slot.block_index = HashTableEntry::BLOCK_INDEX_DELETED;
			}
		}
		Self {
			hash,
			block_count: tables.block.len(),
		}
	}

	pub(super) fn contains(&self, name: &str) -> bool {
		lookup_file_name(&self.hash, self.block_count, name).is_some()
	}
}

pub(super) fn is_live(slot: &HashTableEntry) -> bool {
	!slot.is_free() && !slot.is_deleted()
}

fn check_references(tables: &ClassicTables) -> Result<(), V1LayoutIssue> {
	let mut referenced = vec![false; tables.block.len()];
	for slot in tables.hash.iter().filter(|slot| is_live(slot)) {
		if slot.flags != 0 {
			return Err(V1LayoutIssue::Flags);
		}
		let seen = referenced
			.get_mut(slot.block_index as usize)
			.ok_or(V1LayoutIssue::BlockReferences)?;
		if std::mem::replace(seen, true) {
			return Err(V1LayoutIssue::BlockReferences);
		}
	}
	if referenced.contains(&false) {
		return Err(V1LayoutIssue::BlockReferences);
	}
	Ok(())
}

fn check_payload(blocks: &[BlockTableEntry], payload: &Range<usize>) -> Result<(), V1LayoutIssue> {
	let mut nonempty = Vec::with_capacity(blocks.len());
	for block in blocks {
		if !block.flags.contains(MpqFileFlags::EXISTS)
			|| !RETAINED_FLAGS.contains(block.flags)
			|| block.flags.contains(PACKED)
			|| (block.flags.contains(MpqFileFlags::SECTOR_CRC)
				&& (!block.flags.intersects(PACKED)
					|| block.flags.contains(MpqFileFlags::SINGLE_UNIT)))
		{
			return Err(V1LayoutIssue::Flags);
		}
		let range = packed_range(block);
		if range.start < payload.start
			|| range.end > payload.end
			|| (block.compressed_size == 0) != (block.file_size == 0)
			|| (!block.flags.intersects(PACKED) && block.compressed_size != block.file_size)
			|| (block.flags.contains(MpqFileFlags::SINGLE_UNIT)
				&& block.compressed_size > block.file_size)
		{
			return Err(V1LayoutIssue::PayloadRanges);
		}
		if !range.is_empty() {
			nonempty.push(range);
		}
	}
	nonempty.sort_unstable_by_key(|range| range.start);
	let mut end = payload.start;
	for range in nonempty {
		if range.start != end {
			return Err(V1LayoutIssue::PayloadRanges);
		}
		end = range.end;
	}
	if end != payload.end {
		return Err(V1LayoutIssue::PayloadRanges);
	}
	Ok(())
}

pub(super) fn packed_range(block: &BlockTableEntry) -> Range<usize> {
	slice_range(u64::from(block.file_pos)..block.file_end())
}

fn slice_range(range: Range<u64>) -> Range<usize> {
	// V1 header and block offsets are u32 and supported Rust targets have
	// at least 32-bit addresses. Saturating keeps malformed block ends invalid.
	range.start as usize..usize::try_from(range.end).unwrap_or(usize::MAX)
}
