use std::io::{Read, Seek, SeekFrom};

use flate2::{Decompress, FlushDecompress, Status};
use stormlib_rs::{BlockTableEntry, MpqFileFlags};

use crate::{
	error::{Limit, MpqError},
	header::{Header, le32},
};

const MAP_SCRIPT_NAME: &str = "MapScript.galaxy";
const BASE_MAP_SCRIPT_NAME: &str = "Base.SC2Data\\MapScript.galaxy";
const DOCUMENT_HEADER_NAME: &str = "DocumentHeader";
const MAX_ENTRY_BYTES: u32 = 16 * 1024 * 1024;
const MAX_SECTORS: usize = 4096;
const ZLIB_CODEC: u8 = 0x02;
const SECTOR_OFFSET_BYTES: usize = size_of::<u32>();
const SUPPORTED_FLAGS: MpqFileFlags = MpqFileFlags::EXISTS
	.union(MpqFileFlags::SINGLE_UNIT)
	.union(MpqFileFlags::COMPRESS);

/// Archive members read for local map metadata inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataEntry {
	/// Galaxy script at the archive root.
	MapScript,
	/// Galaxy script in Base.SC2Data.
	BaseMapScript,
	/// Binary map document metadata.
	DocumentHeader,
}

impl MetadataEntry {
	pub(crate) fn name(self) -> &'static str {
		match self {
			Self::MapScript => MAP_SCRIPT_NAME,
			Self::BaseMapScript => BASE_MAP_SCRIPT_NAME,
			Self::DocumentHeader => DOCUMENT_HEADER_NAME,
		}
	}

	pub(crate) fn read(
		self,
		reader: &mut (impl Read + Seek),
		header: &Header,
		block: BlockTableEntry,
	) -> Result<Vec<u8>, MpqError> {
		if !block.flags.contains(MpqFileFlags::EXISTS)
			|| block.flags.bits() & !SUPPORTED_FLAGS.bits() != 0
		{
			return Err(MpqError::UnsupportedEntry(self));
		}
		if block.compressed_size > MAX_ENTRY_BYTES || block.file_size > MAX_ENTRY_BYTES {
			return Err(MpqError::LimitExceeded(Limit::EntryBytes));
		}
		let start = u64::from(block.file_pos);
		if start < header.payload.start || block.file_end() > header.payload.end {
			return Err(MpqError::InvalidEntry(self));
		}
		let packed = block.compressed_size as usize;
		let expanded = block.file_size as usize;
		let compressed = block.flags.contains(MpqFileFlags::COMPRESS);
		let single = block.flags.contains(MpqFileFlags::SINGLE_UNIT);
		if (!compressed && packed != expanded) || (single && packed > expanded) {
			return Err(MpqError::InvalidEntry(self));
		}

		// Empty sector-based entries have no payload or sector-offset table.
		if packed == 0 && expanded == 0 {
			return Ok(Vec::new());
		}

		let sectors = expanded.div_ceil(header.sector_bytes);
		if compressed && !single && sectors > MAX_SECTORS {
			return Err(MpqError::LimitExceeded(Limit::EntrySectors));
		}
		let mut input = vec![0; packed];
		reader.seek(SeekFrom::Start(start))?;
		reader.read_exact(&mut input)?;
		if !compressed || single && packed == expanded {
			return Ok(input);
		}
		let mut output = vec![0; expanded + 1];
		if single {
			self.decode_sector(&input, &mut output)?;
		} else {
			self.decode_sectors(&input, &mut output, header.sector_bytes, sectors)?;
		}
		output.truncate(expanded);
		Ok(output)
	}

	fn decode_sectors(
		self,
		input: &[u8],
		output: &mut [u8],
		sector_bytes: usize,
		sectors: usize,
	) -> Result<(), MpqError> {
		let expanded = output.len() - 1;
		let table_len = (sectors + 1) * SECTOR_OFFSET_BYTES;
		let table = input.get(..table_len).ok_or(MpqError::InvalidEntry(self))?;
		let mut offsets = table
			.as_chunks::<SECTOR_OFFSET_BYTES>()
			.0
			.iter()
			.map(|bytes| le32(bytes) as usize);
		let mut start = offsets.next().ok_or(MpqError::InvalidEntry(self))?;
		if start != table_len {
			return Err(MpqError::InvalidEntry(self));
		}

		for (index, end) in offsets.enumerate() {
			let sector = input.get(start..end).ok_or(MpqError::InvalidEntry(self))?;
			let output_start = index * sector_bytes;
			let output_end = (output_start + sector_bytes).min(expanded);
			self.decode_sector(sector, &mut output[output_start..=output_end])?;
			start = end;
		}
		if start != input.len() {
			return Err(MpqError::InvalidEntry(self));
		}
		Ok(())
	}

	// The final output byte is spare capacity to distinguish exact-sized streams
	// from streams with more expanded data than the archive declares.
	fn decode_sector(self, input: &[u8], output: &mut [u8]) -> Result<(), MpqError> {
		let expected = output.len() - 1;
		if input.len() == expected {
			output[..expected].copy_from_slice(input);
			return Ok(());
		}
		if input.is_empty() || input.len() > expected {
			return Err(MpqError::InvalidEntry(self));
		}
		let (codec, compressed) = input.split_first().ok_or(MpqError::InvalidEntry(self))?;
		if *codec != ZLIB_CODEC {
			return Err(MpqError::UnsupportedEntry(self));
		}
		let mut inflater = Decompress::new(true);
		let status = inflater
			.decompress(compressed, output, FlushDecompress::Finish)
			.map_err(|_| MpqError::InvalidEntry(self))?;
		if status != Status::StreamEnd
			|| inflater.total_in() != compressed.len() as u64
			|| inflater.total_out() != expected as u64
		{
			return Err(MpqError::InvalidEntry(self));
		}
		Ok(())
	}
}
