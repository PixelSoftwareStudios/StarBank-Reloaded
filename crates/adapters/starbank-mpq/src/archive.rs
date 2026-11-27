use std::{
	fmt,
	io::{Read, Seek, SeekFrom},
	ops::Range,
};

use stormlib_rs::{
	BlockTableEntry, HashTableEntry, MpqFileFlags, decrypt_mpq_block, derive_file_key,
	lookup_file_name,
};

use crate::{
	entry::MetadataEntry,
	error::MpqError,
	header::{Header, HeaderLocation, le32},
};

const HASH_TABLE_NAME: &str = "(hash table)";
const BLOCK_TABLE_NAME: &str = "(block table)";

/// One open source with bounded access to known map metadata entry names.
///
/// The caller is responsible for authorizing the source and opening it as a regular
/// file if using a filesystem. Opening the archive reads its header and classic tables.
pub struct MetadataArchive<R> {
	reader: R,
	header: Header,
	tables: ClassicTables,
}

impl<R> fmt::Debug for MetadataArchive<R> {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("MetadataArchive")
			.field("hash_entries", &self.tables.hash.len())
			.field("block_entries", &self.tables.block.len())
			.finish_non_exhaustive()
	}
}

impl<R: Read + Seek> MetadataArchive<R> {
	/// Reads the classic lookup tables from a seekable source.
	///
	/// # Errors
	///
	/// Returns an error for unreadable sources, inconsistent ranges, unsupported
	/// layouts or limit violations. The reader is consumed even on failure.
	pub fn open(mut reader: R) -> Result<Self, MpqError> {
		let header = Header::read(&mut reader, HeaderLocation::AllowUserData)?;
		let tables = ClassicTables::read(&mut reader, &header)?;
		Ok(Self {
			reader,
			header,
			tables,
		})
	}

	/// Reads one known entry into memory, returning `None` when its name is absent.
	///
	/// # Errors
	///
	/// Returns an error if the entry cannot be read, uses unsupported storage,
	/// exceeds a resource limit or has an inconsistent payload.
	pub fn read(&mut self, entry: MetadataEntry) -> Result<Option<Vec<u8>>, MpqError> {
		let Some(found) =
			lookup_file_name(&self.tables.hash, self.tables.block.len(), entry.name())
		else {
			return Ok(None);
		};
		entry
			.read(
				&mut self.reader,
				&self.header,
				self.tables.block[found.block_index],
			)
			.map(Some)
	}
}

pub(crate) struct ClassicTables {
	pub(crate) hash: Vec<HashTableEntry>,
	pub(crate) block: Vec<BlockTableEntry>,
}

impl ClassicTables {
	pub(crate) fn read(reader: &mut (impl Read + Seek), header: &Header) -> Result<Self, MpqError> {
		let hash_bytes = read_table(reader, header, &header.hash_table, HASH_TABLE_NAME)?;
		let hash_table = hash_bytes
			.as_chunks::<{ HashTableEntry::SERIALIZED_LEN }>()
			.0
			.iter()
			.map(|raw| HashTableEntry {
				hash_a: le32(&raw[0..4]),
				hash_b: le32(&raw[4..8]),
				locale: u16::from_le_bytes([raw[8], raw[9]]),
				platform: raw[10],
				flags: raw[11],
				block_index: le32(&raw[12..16]),
			})
			.collect::<Vec<_>>();
		drop(hash_bytes);
		let block_bytes = read_table(reader, header, &header.block_table, BLOCK_TABLE_NAME)?;
		let block_table = block_bytes
			.as_chunks::<{ BlockTableEntry::SERIALIZED_LEN }>()
			.0
			.iter()
			.map(|raw| BlockTableEntry {
				file_pos: le32(&raw[0..4]),
				compressed_size: le32(&raw[4..8]),
				file_size: le32(&raw[8..12]),
				flags: MpqFileFlags::from_bits_retain(le32(&raw[12..16])),
			})
			.collect::<Vec<_>>();
		if hash_table.iter().any(|slot| {
			!slot.is_free() && !slot.is_deleted() && slot.block_index as usize >= block_table.len()
		}) {
			return Err(MpqError::InvalidArchive(
				"hash table references a missing block",
			));
		}
		Ok(Self {
			hash: hash_table,
			block: block_table,
		})
	}
}

fn read_table(
	reader: &mut (impl Read + Seek),
	header: &Header,
	range: &Range<u64>,
	table_name: &str,
) -> Result<Vec<u8>, MpqError> {
	let mut bytes = vec![0; (range.end - range.start) as usize];
	reader.seek(SeekFrom::Start(header.source_offset(range.start)?))?;
	reader.read_exact(&mut bytes)?;
	decrypt_mpq_block(&mut bytes, derive_file_key(table_name, 0, 0, false));
	Ok(bytes)
}
