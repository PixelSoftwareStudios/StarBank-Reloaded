use std::{
	io::{Read, Seek, SeekFrom},
	ops::Range,
};

use stormlib_rs::{BlockTableEntry, HashTableEntry};

use crate::error::{Limit, MpqError};

const MPQ_SIGNATURE: &[u8; 4] = b"MPQ\x1a";
const USER_DATA_SIGNATURE: &[u8; 4] = b"MPQ\x1b";
const VERSION_V1: u16 = 0;
const VERSION_V2: u16 = 1;
const VERSION_V4: u16 = 3;
const V1_HEADER_BYTES: usize = 32;
const V2_HEADER_BYTES: usize = 44;
const V4_HEADER_BYTES: usize = 208;
const HEADER_LEN_OFFSET: usize = 4;
const ARCHIVE_LEN_OFFSET: usize = 8;
const VERSION_OFFSET: usize = 12;
const SECTOR_SHIFT_OFFSET: usize = 14;
const HASH_OFFSET: usize = 16;
const BLOCK_OFFSET: usize = 20;
const HASH_COUNT_OFFSET: usize = 24;
const BLOCK_COUNT_OFFSET: usize = 28;
const HIGH_BLOCK_TABLE_OFFSET: usize = 32;
const HIGH_CLASSIC_OFFSETS: usize = 40;
const ARCHIVE_LEN_64_OFFSET: usize = 44;
const BET_OFFSET: usize = 52;
const HET_OFFSET: usize = 60;
const HASH_BYTES_OFFSET: usize = 68;
const BLOCK_BYTES_OFFSET: usize = 76;
const HET_BYTES_OFFSET: usize = 92;
const BET_BYTES_OFFSET: usize = 100;
const SECTOR_BYTES_BASE: usize = 512;
const MAX_SECTOR_SHIFT: u16 = 7;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TABLE_ENTRIES: u32 = 64 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct Header {
	pub(crate) hash_table: Range<u64>,
	pub(crate) block_table: Range<u64>,
	pub(crate) payload: Range<u64>,
	pub(crate) sector_bytes: usize,
}

impl Header {
	pub(crate) fn read(reader: &mut (impl Read + Seek)) -> Result<Self, MpqError> {
		let file_len = reader.seek(SeekFrom::End(0))?;
		if file_len > MAX_ARCHIVE_BYTES {
			return Err(MpqError::LimitExceeded(Limit::ArchiveBytes));
		}
		let mut raw = [0; V4_HEADER_BYTES];
		if file_len < V1_HEADER_BYTES as u64 {
			return Err(MpqError::InvalidArchive("truncated header"));
		}
		reader.seek(SeekFrom::Start(0))?;
		reader.read_exact(&mut raw[..V1_HEADER_BYTES])?;
		if raw.starts_with(USER_DATA_SIGNATURE) {
			return Err(MpqError::UnsupportedArchive("user data header"));
		}
		if !raw.starts_with(MPQ_SIGNATURE) {
			return Err(MpqError::InvalidArchive(
				"missing archive signature at offset zero",
			));
		}
		let header_len = u32_at(&raw, HEADER_LEN_OFFSET);
		let version = u16_at(&raw, VERSION_OFFSET);
		let expected_len = match version {
			VERSION_V1 => V1_HEADER_BYTES,
			VERSION_V2 => V2_HEADER_BYTES,
			VERSION_V4 => V4_HEADER_BYTES,
			_ => return Err(MpqError::UnsupportedArchive("header version")),
		};
		if u64::from(header_len) > file_len {
			return Err(MpqError::InvalidArchive("truncated extended header"));
		}
		if header_len as usize != expected_len {
			return Err(MpqError::UnsupportedArchive("header length for version"));
		}
		reader.read_exact(&mut raw[V1_HEADER_BYTES..expected_len])?;

		let archive_len = if version == VERSION_V4 {
			u64_at(&raw, ARCHIVE_LEN_64_OFFSET)
		} else {
			u64::from(u32_at(&raw, ARCHIVE_LEN_OFFSET))
		};
		let sector_shift = u16_at(&raw, SECTOR_SHIFT_OFFSET);
		if archive_len > file_len
			|| archive_len < u64::from(header_len)
			|| sector_shift > MAX_SECTOR_SHIFT
		{
			return Err(MpqError::InvalidArchive("archive length or sector size"));
		}

		if version >= VERSION_V2
			&& (u64_at(&raw, HIGH_BLOCK_TABLE_OFFSET) != 0
				|| u32_at(&raw, HIGH_CLASSIC_OFFSETS) != 0)
		{
			return Err(MpqError::UnsupportedArchive(
				"high block table or extended classic table offsets",
			));
		}
		let hash_count = u32_at(&raw, HASH_COUNT_OFFSET);
		let block_count = u32_at(&raw, BLOCK_COUNT_OFFSET);
		if hash_count == 0 || block_count == 0 {
			return Err(MpqError::UnsupportedArchive("missing classic MPQ tables"));
		}
		if !hash_count.is_power_of_two() {
			return Err(MpqError::InvalidArchive("hash table count"));
		}
		if hash_count > MAX_TABLE_ENTRIES || block_count > MAX_TABLE_ENTRIES {
			return Err(MpqError::LimitExceeded(Limit::TableEntries));
		}
		let hash_bytes = u64::from(hash_count) * HashTableEntry::SERIALIZED_LEN as u64;
		let block_bytes = u64::from(block_count) * BlockTableEntry::SERIALIZED_LEN as u64;
		if version == VERSION_V4
			&& (u64_at(&raw, HASH_BYTES_OFFSET) != hash_bytes
				|| u64_at(&raw, BLOCK_BYTES_OFFSET) != block_bytes)
		{
			return Err(MpqError::UnsupportedArchive(
				"compressed classic MPQ tables",
			));
		}
		let hash_start = u64::from(u32_at(&raw, HASH_OFFSET));
		let block_start = u64::from(u32_at(&raw, BLOCK_OFFSET));
		let hash_table = hash_start..hash_start + hash_bytes;
		let block_table = block_start..block_start + block_bytes;
		let content_start = u64::from(header_len);
		if hash_table.start < content_start
			|| hash_table.end > block_table.start
			|| block_table.end > archive_len
		{
			return Err(MpqError::InvalidArchive("classic table ranges"));
		}
		if version < VERSION_V4
			&& (hash_table.end != block_table.start || block_table.end != archive_len)
		{
			return Err(MpqError::UnsupportedArchive(
				"non-contiguous classic MPQ tables",
			));
		}
		let mut content_end = hash_table.start;
		if version == VERSION_V4 {
			let het = extended_table_range(&raw, HET_OFFSET, HET_BYTES_OFFSET)?;
			let bet = extended_table_range(&raw, BET_OFFSET, BET_BYTES_OFFSET)?;
			for table in [&het, &bet].into_iter().flatten() {
				if table.start < content_start || table.end > hash_table.start {
					return Err(MpqError::InvalidArchive("extended table range"));
				}
				content_end = content_end.min(table.start);
			}
			if let (Some(het), Some(bet)) = (&het, &bet)
				&& het.end > bet.start
			{
				return Err(MpqError::InvalidArchive("extended table order"));
			}
		}
		Ok(Self {
			hash_table,
			block_table,
			payload: content_start..content_end,
			sector_bytes: SECTOR_BYTES_BASE << sector_shift,
		})
	}
}

fn extended_table_range(
	raw: &[u8; V4_HEADER_BYTES],
	offset_field: usize,
	size_field: usize,
) -> Result<Option<Range<u64>>, MpqError> {
	let offset = u64_at(raw, offset_field);
	let size = u64_at(raw, size_field);
	match (offset, size) {
		(0, 0) => Ok(None),
		(0, _) | (_, 0) => Err(MpqError::InvalidArchive("incomplete extended table range")),
		_ => {
			let end = offset
				.checked_add(size)
				.ok_or(MpqError::InvalidArchive("extended table range"))?;
			Ok(Some(offset..end))
		}
	}
}

fn u16_at(raw: &[u8; V4_HEADER_BYTES], index: usize) -> u16 {
	u16::from_le_bytes([raw[index], raw[index + 1]])
}

fn u32_at(raw: &[u8; V4_HEADER_BYTES], index: usize) -> u32 {
	le32(&raw[index..index + size_of::<u32>()])
}

fn u64_at(raw: &[u8; V4_HEADER_BYTES], index: usize) -> u64 {
	u64::from(u32_at(raw, index)) | (u64::from(u32_at(raw, index + 4)) << u32::BITS)
}
/// Callers supply a four-byte chunk or a four-byte range from a fixed header.
pub(crate) fn le32(bytes: &[u8]) -> u32 {
	u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
