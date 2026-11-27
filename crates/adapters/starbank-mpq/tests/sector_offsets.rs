use std::{io::Cursor, io::Write as _};

use flate2::{Compression, write::ZlibEncoder};
use starbank_mpq::{MetadataArchive, MetadataEntry, MpqError};
use stormlib_rs::{MpqFileFlags, decrypt_mpq_block, derive_file_key, encrypt_mpq_block};

const TEMPLATE: &[u8] = include_bytes!("fixtures/empty-script.s2ma");
const SECTOR_SHIFT: u16 = 3;
const SECTOR_BYTES: usize = 512 << SECTOR_SHIFT;
const OFFSET_BYTES: usize = size_of::<u32>();
const ZLIB_CODEC: u8 = 2;
const OPAQUE_BYTE: u8 = 0x5a;
const MAX_TABLE_PADDING: usize = 1024;
const PADDED_TABLE_BYTES: [usize; 3] = [OFFSET_BYTES, 2 * OFFSET_BYTES, MAX_TABLE_PADDING];

const V4_HEADER_BYTES: usize = 208;
const V4_VERSION: u16 = 3;
const HASH_START: usize = 2048;
const HASH_COUNT: usize = 16;
const HASH_BYTES: usize = HASH_COUNT * 16;
const BLOCK_START: usize = HASH_START + HASH_BYTES;
const BLOCK_BYTES: usize = 16;
const ARCHIVE_BYTES: usize = BLOCK_START + BLOCK_BYTES;
// V4 requires the MD5 of the preceding 192 header bytes. The fixed classic table
// positions leave room for each padding variant without changing this header.
const HEADER_MD5: [u8; 16] = [
	0x3d, 0xc3, 0xeb, 0x81, 0xa7, 0x95, 0x5e, 0x75, 0xb8, 0x9e, 0xd3, 0x63, 0xe9, 0xd6, 0x32, 0x7a,
];

pub fn authored_text() -> Vec<u8> {
	b"Standalone authored archive probe.\n".repeat(160)
}

pub fn padded_archive(padding: usize) -> Vec<u8> {
	let expected = authored_text();
	let total_offsets = expected.len().div_ceil(SECTOR_BYTES) + 1;
	let first_offset = total_offsets * OFFSET_BYTES + padding;
	let mut packed = vec![OPAQUE_BYTE; first_offset];
	put_u32(&mut packed, 0, first_offset);
	for (sector_index, sector) in expected.chunks(SECTOR_BYTES).enumerate() {
		let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
		encoder.write_all(sector).unwrap();
		let compressed = encoder.finish().unwrap();
		assert!(compressed.len() + 1 < sector.len());
		packed.push(ZLIB_CODEC);
		packed.extend(compressed);
		let end = packed.len();
		put_u32(&mut packed, (sector_index + 1) * OFFSET_BYTES, end);
	}
	assert!(V4_HEADER_BYTES + packed.len() <= HASH_START);

	let mut archive = vec![0; ARCHIVE_BYTES];
	archive[..4].copy_from_slice(b"MPQ\x1a");
	put_u32(&mut archive, 4, V4_HEADER_BYTES);
	put_u32(&mut archive, 8, ARCHIVE_BYTES);
	archive[12..14].copy_from_slice(&V4_VERSION.to_le_bytes());
	archive[14..16].copy_from_slice(&SECTOR_SHIFT.to_le_bytes());
	put_u32(&mut archive, 16, HASH_START);
	put_u32(&mut archive, 20, BLOCK_START);
	put_u32(&mut archive, 24, HASH_COUNT);
	put_u32(&mut archive, 28, 1);
	archive[44..52].copy_from_slice(&(ARCHIVE_BYTES as u64).to_le_bytes());
	archive[68..76].copy_from_slice(&(HASH_BYTES as u64).to_le_bytes());
	archive[76..84].copy_from_slice(&(BLOCK_BYTES as u64).to_le_bytes());
	archive[V4_HEADER_BYTES - HEADER_MD5.len()..V4_HEADER_BYTES].copy_from_slice(&HEADER_MD5);
	archive[V4_HEADER_BYTES..V4_HEADER_BYTES + packed.len()].copy_from_slice(&packed);

	// Reuse the sole MapScript hash entry from the authored StormLib fixture.
	// Its hash positions and block index stay valid in the new classic-only V4 archive.
	let template_hash = u32::from_le_bytes(TEMPLATE[16..20].try_into().unwrap()) as usize;
	assert_eq!(
		u32::from_le_bytes(TEMPLATE[24..28].try_into().unwrap()) as usize,
		HASH_COUNT
	);
	assert_eq!(u32::from_le_bytes(TEMPLATE[28..32].try_into().unwrap()), 1);
	archive[HASH_START..BLOCK_START]
		.copy_from_slice(&TEMPLATE[template_hash..template_hash + HASH_BYTES]);
	let block = &mut archive[BLOCK_START..];
	put_u32(block, 0, V4_HEADER_BYTES);
	put_u32(block, 4, packed.len());
	put_u32(block, 8, expected.len());
	let flags = MpqFileFlags::EXISTS | MpqFileFlags::COMPRESS;
	block[12..16].copy_from_slice(&flags.bits().to_le_bytes());
	encrypt_mpq_block(block, derive_file_key("(block table)", 0, 0, false));
	archive
}

fn put_u32(bytes: &mut [u8], offset: usize, value: usize) {
	let word = u32::try_from(value).unwrap().to_le_bytes();
	bytes[offset..offset + word.len()].copy_from_slice(&word);
}

#[test]
fn aligned_opaque_sector_padding_preserves_the_authored_payload() {
	let expected = authored_text();
	for padding in PADDED_TABLE_BYTES {
		let mut archive = MetadataArchive::open(Cursor::new(padded_archive(padding))).unwrap();
		assert_eq!(
			archive.read(MetadataEntry::MapScript).unwrap().as_deref(),
			Some(expected.as_slice()),
			"opaque sector table padding of {padding} bytes",
		);
	}
}

fn packed_length(archive: &[u8]) -> usize {
	let mut block = archive[BLOCK_START..].to_vec();
	decrypt_mpq_block(&mut block, derive_file_key("(block table)", 0, 0, false));
	u32::from_le_bytes(block[4..8].try_into().unwrap()) as usize
}

fn extend_packed_length(archive: &mut [u8], additional_bytes: usize) {
	let block = &mut archive[BLOCK_START..];
	let key = derive_file_key("(block table)", 0, 0, false);
	decrypt_mpq_block(block, key);
	let length = u32::from_le_bytes(block[4..8].try_into().unwrap()) as usize;
	put_u32(block, 4, length + additional_bytes);
	encrypt_mpq_block(block, key);
}

fn assert_invalid_entry(bytes: Vec<u8>) {
	let mut archive = MetadataArchive::open(Cursor::new(bytes)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::MapScript),
		Err(MpqError::InvalidEntry(MetadataEntry::MapScript))
	));
}

#[test]
fn the_first_sector_must_follow_a_bounded_word_aligned_table() {
	let minimum = (authored_text().len().div_ceil(SECTOR_BYTES) + 1) * OFFSET_BYTES;
	for first in [
		minimum - OFFSET_BYTES,
		minimum + OFFSET_BYTES / 2,
		minimum + MAX_TABLE_PADDING + OFFSET_BYTES,
		u32::MAX as usize,
	] {
		let mut bytes = padded_archive(MAX_TABLE_PADDING);
		put_u32(&mut bytes, V4_HEADER_BYTES, first);
		assert_invalid_entry(bytes);
	}
	let mut bytes = padded_archive(4);
	let outside = packed_length(&bytes) + OFFSET_BYTES;
	put_u32(&mut bytes, V4_HEADER_BYTES, outside);
	assert_invalid_entry(bytes);
}

#[test]
fn sector_ranges_still_have_to_progress_and_fit_the_packed_payload() {
	let original = padded_archive(8);
	let first = u32::from_le_bytes(
		original[V4_HEADER_BYTES..V4_HEADER_BYTES + OFFSET_BYTES]
			.try_into()
			.unwrap(),
	) as usize;
	let packed = packed_length(&original);
	for end in [first - 1, first, packed + 1] {
		let mut bytes = original.clone();
		put_u32(&mut bytes, V4_HEADER_BYTES + OFFSET_BYTES, end);
		assert_invalid_entry(bytes);
	}
}

#[test]
fn padding_does_not_allow_trailing_bytes_or_a_truncated_final_sector() {
	let mut trailing = padded_archive(8);
	extend_packed_length(&mut trailing, OFFSET_BYTES);
	assert_invalid_entry(trailing);

	let mut truncated = padded_archive(8);
	let packed = packed_length(&truncated);
	let last_offset = authored_text().len().div_ceil(SECTOR_BYTES) * OFFSET_BYTES;
	put_u32(&mut truncated, V4_HEADER_BYTES + last_offset, packed - 1);
	assert_invalid_entry(truncated);
}

#[test]
fn padded_sectors_are_read_relative_to_the_declared_archive_origin() {
	const DECLARED_ORIGIN: usize = 512;
	let mut bytes = vec![0; DECLARED_ORIGIN];
	bytes[..4].copy_from_slice(b"MPQ\x1b");
	put_u32(&mut bytes, 4, 16);
	put_u32(&mut bytes, 8, DECLARED_ORIGIN);
	bytes.extend(padded_archive(MAX_TABLE_PADDING));
	let mut archive = MetadataArchive::open(Cursor::new(bytes)).unwrap();
	assert_eq!(
		archive.read(MetadataEntry::MapScript).unwrap(),
		Some(authored_text())
	);
}
