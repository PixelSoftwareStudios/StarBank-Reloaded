use std::io::{self, Cursor, Read, Seek, SeekFrom};

use starbank_mpq::{
	Limit, MetadataArchive, MetadataEntry, MpqError, V1Addition, V1AppendError, V1AppendPlan,
};
use stormlib_rs::{derive_file_key, encrypt_mpq_block};

const PREFIX_BYTES: usize = 16;
const MPQ_MAGIC: &[u8; 4] = b"MPQ\x1a";
const USER_MAGIC: &[u8; 4] = b"MPQ\x1b";
const HEADER_LENGTH: usize = 4;
const ARCHIVE_LENGTH: usize = 8;
const HASH_POSITION: usize = 16;
const BLOCK_POSITION: usize = 20;
const HASH_COUNT: usize = 8;
const RECORD_BYTES: usize = 16;
const STORED_SINGLE_UNIT: u32 = 0x8100_0000;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const ENTRIES: [MetadataEntry; 3] = [
	MetadataEntry::MapScript,
	MetadataEntry::BaseMapScript,
	MetadataEntry::DocumentHeader,
];
const CONTENT: [&[u8]; 3] = [
	b"// Name: Prefix fixture\n",
	b"// Author: Independent base\n",
	b"",
];
// Fixed values calculated independently from the published MPQ filename-hash algorithm.
const NAME_HASHES: [(usize, u32, u32); 3] = [
	(5, 0x387d_cd50, 0x166f_1c7d),
	(6, 0xb866_fc7b, 0x433d_55cd),
	(4, 0x5e89_7e7d, 0x5951_aebe),
];
const APPEND_SOURCE: &[u8] = include_bytes!("fixtures/append-v1-source.s2ma");
const APPEND_OUTPUT: &[u8] = include_bytes!("fixtures/append-v1-output.s2ma");
const ADDITIONS: [V1Addition<'static>; 2] = [
	V1Addition {
		name: "Fixture\\Restored.txt",
		bytes: b"Synthetic restored component\n",
	},
	V1Addition {
		name: "Fixture\\Supplement.txt",
		bytes: b"Synthetic supplemental component\n",
	},
];

#[derive(Clone, Copy, Debug)]
enum Version {
	V1,
	V2,
	V4,
}

impl Version {
	fn fields(self) -> (u16, usize) {
		match self {
			Self::V1 => (0, 32),
			Self::V2 => (1, 44),
			Self::V4 => (3, 208),
		}
	}
}

// Authored bytes, not a rewritten private archive or the reader's header encoder.
fn archive(version: Version) -> Vec<u8> {
	let (tag, header_bytes) = version.fields();
	let mut bytes = vec![0; header_bytes];
	bytes[..4].copy_from_slice(MPQ_MAGIC);
	put32(&mut bytes, HEADER_LENGTH, header_bytes);
	bytes[12..14].copy_from_slice(&tag.to_le_bytes());
	bytes[14..16].copy_from_slice(&3u16.to_le_bytes());
	let mut blocks = vec![0; ENTRIES.len() * RECORD_BYTES];
	let mut hashes = vec![u8::MAX; HASH_COUNT * RECORD_BYTES];
	for (index, content) in CONTENT.into_iter().enumerate() {
		let block = &mut blocks[index * RECORD_BYTES..(index + 1) * RECORD_BYTES];
		put32(block, 0, bytes.len());
		put32(block, 4, content.len());
		put32(block, 8, content.len());
		block[12..16].copy_from_slice(&STORED_SINGLE_UNIT.to_le_bytes());
		bytes.extend_from_slice(content);
		let (slot, hash_a, hash_b) = NAME_HASHES[index];
		let hash = &mut hashes[slot * RECORD_BYTES..(slot + 1) * RECORD_BYTES];
		hash.fill(0);
		hash[..4].copy_from_slice(&hash_a.to_le_bytes());
		hash[4..8].copy_from_slice(&hash_b.to_le_bytes());
		put32(hash, 12, index);
	}
	let hash_start = bytes.len();
	encrypt_mpq_block(&mut hashes, derive_file_key("(hash table)", 0, 0, false));
	bytes.extend_from_slice(&hashes);
	let block_start = bytes.len();
	encrypt_mpq_block(&mut blocks, derive_file_key("(block table)", 0, 0, false));
	bytes.extend_from_slice(&blocks);
	let archive_bytes = bytes.len();
	put32(&mut bytes, ARCHIVE_LENGTH, archive_bytes);
	put32(&mut bytes, HASH_POSITION, hash_start);
	put32(&mut bytes, BLOCK_POSITION, block_start);
	put32(&mut bytes, 24, HASH_COUNT);
	put32(&mut bytes, 28, ENTRIES.len());
	if matches!(version, Version::V4) {
		bytes[44..52].copy_from_slice(&(archive_bytes as u64).to_le_bytes());
		bytes[68..76].copy_from_slice(&(hashes.len() as u64).to_le_bytes());
		bytes[76..84].copy_from_slice(&(blocks.len() as u64).to_le_bytes());
	}
	bytes
}

fn wrapped(inner: &[u8], base: usize, header_field: usize) -> Vec<u8> {
	assert!(base >= PREFIX_BYTES);
	let mut bytes = vec![0xa5; base];
	bytes[..4].copy_from_slice(USER_MAGIC);
	put32(&mut bytes, 4, base);
	put32(&mut bytes, 8, base);
	put32(&mut bytes, 12, header_field);
	bytes.extend_from_slice(inner);
	bytes
}

fn put32(bytes: &mut [u8], offset: usize, value: usize) {
	bytes[offset..offset + 4].copy_from_slice(&u32::try_from(value).unwrap().to_le_bytes());
}

fn assert_entries(bytes: &[u8]) {
	let mut reader = MetadataArchive::open(Cursor::new(bytes)).expect("synthetic metadata archive");
	for (entry, expected) in ENTRIES.into_iter().zip(CONTENT) {
		assert_eq!(
			reader.read(entry).unwrap().as_deref(),
			Some(expected),
			"{entry:?}"
		);
	}
}

fn assert_invalid(bytes: &[u8]) {
	assert!(matches!(
		MetadataArchive::open(Cursor::new(bytes)),
		Err(MpqError::InvalidArchive(_))
	));
}

#[test]
fn flat_and_declared_prefix_v1_v2_v4_return_the_same_three_typed_entries() {
	for version in [Version::V1, Version::V2, Version::V4] {
		let inner = archive(version);
		assert_entries(&inner);
		for base in [PREFIX_BYTES, PREFIX_BYTES + 1, 512] {
			for header_field in [0, PREFIX_BYTES] {
				assert_entries(&wrapped(&inner, base, header_field));
			}
		}
	}
}

#[test]
fn stored_and_sector_compressed_stormlib_fixtures_keep_all_three_results() {
	for inner in [
		include_bytes!("fixtures/v1-no-list.s2ma").as_slice(),
		include_bytes!("fixtures/v2-list.s2ma").as_slice(),
		include_bytes!("fixtures/v4-no-list.s2ma").as_slice(),
	] {
		let mut flat = MetadataArchive::open(Cursor::new(inner)).unwrap();
		let expected = ENTRIES.map(|entry| flat.read(entry).unwrap());
		for base in [733, 1024] {
			for header_field in [0, PREFIX_BYTES] {
				let bytes = wrapped(inner, base, header_field);
				let mut reader = MetadataArchive::open(Cursor::new(&bytes)).unwrap();
				for (entry, expected) in ENTRIES.into_iter().zip(&expected) {
					assert_eq!(&reader.read(entry).unwrap(), expected);
				}
			}
		}
	}
}

#[test]
fn zero_sized_user_data_and_cursor_start_position_do_not_change_the_declared_origin() {
	let mut bytes = wrapped(&archive(Version::V1), PREFIX_BYTES, 0);
	put32(&mut bytes, 4, 0);
	let mut cursor = Cursor::new(&bytes);
	cursor.set_position(7);
	let mut reader = MetadataArchive::open(cursor).unwrap();
	for (entry, expected) in ENTRIES.into_iter().zip(CONTENT) {
		assert_eq!(reader.read(entry).unwrap().as_deref(), Some(expected));
	}
}

#[test]
fn every_truncated_prefix_refuses_before_reading_any_entry() {
	for version in [Version::V1, Version::V2, Version::V4] {
		let bytes = wrapped(&archive(version), 17, 0);
		for end in 0..bytes.len() {
			assert_invalid(&bytes[..end]);
		}
	}
}

#[test]
fn malformed_declared_sizes_and_bases_are_not_followed() {
	let good = wrapped(&archive(Version::V1), 512, 0);
	for (field, value) in [
		(8, 0),
		(8, 15),
		(8, good.len()),
		(8, u32::MAX as usize),
		(4, 513),
		(12, 513),
	] {
		let mut bytes = good.clone();
		put32(&mut bytes, field, value);
		assert_invalid(&bytes);
	}
	let mut oversized_header = good.clone();
	put32(&mut oversized_header, 4, 16);
	put32(&mut oversized_header, 12, 17);
	assert_invalid(&oversized_header);
}

#[test]
fn declared_target_and_extended_header_must_fit_even_when_relative_lengths_fit_the_file() {
	for version in [Version::V1, Version::V2, Version::V4] {
		let inner = archive(version);
		let mut bytes = wrapped(&inner, 512, 0);
		let relative_length = bytes.len();
		put32(&mut bytes, 512 + ARCHIVE_LENGTH, relative_length);
		if matches!(version, Version::V4) {
			bytes[512 + 44..512 + 52].copy_from_slice(&(relative_length as u64).to_le_bytes());
		}
		assert_invalid(&bytes);
		let (_, header_bytes) = version.fields();
		if header_bytes > 32 {
			assert_invalid(&wrapped(&inner[..32], 16, 0));
		}
	}
	let mut overflow = wrapped(&archive(Version::V4), 17, 0);
	overflow[17 + 44..17 + 52].copy_from_slice(&u64::MAX.to_le_bytes());
	assert_invalid(&overflow);
}

#[test]
fn signatures_outside_the_declared_target_are_not_scanned_or_recursed() {
	let inner = archive(Version::V1);
	let mut bytes = wrapped(&inner, 512, 0);
	bytes[32..32 + inner.len()].copy_from_slice(&inner);
	bytes[512..516].copy_from_slice(b"NONE");
	assert_invalid(&bytes);
	assert_invalid(&wrapped(&wrapped(&inner, 16, 0), 16, 0));
	let mut unmarked = vec![0; 512];
	unmarked.extend_from_slice(&inner);
	assert_invalid(&unmarked);
}

#[test]
fn relative_entry_extent_cannot_alias_the_user_prefix_or_mpq_header() {
	let mut inner = archive(Version::V1);
	let offset = u32::from_le_bytes(inner[20..24].try_into().unwrap()) as usize;
	let table = &mut inner[offset..];
	let key = derive_file_key("(block table)", 0, 0, false);
	stormlib_rs::decrypt_mpq_block(table, key);
	for block in table.as_chunks_mut::<RECORD_BYTES>().0 {
		put32(block, 0, 0);
	}
	encrypt_mpq_block(table, key);
	let bytes = wrapped(&inner, 512, 0);
	let mut reader = MetadataArchive::open(Cursor::new(&bytes)).unwrap();
	for entry in ENTRIES {
		assert!(
			matches!(reader.read(entry), Err(MpqError::InvalidEntry(actual)) if actual == entry)
		);
	}
}

#[test]
fn both_append_source_and_output_keep_the_zero_origin_requirement() {
	let plan = V1AppendPlan::prepare(APPEND_SOURCE, &ADDITIONS).unwrap();
	plan.verify(APPEND_OUTPUT).unwrap();
	let independent = archive(Version::V1);
	assert!(V1AppendPlan::prepare(&independent, &ADDITIONS[..1]).is_ok());
	for base in [PREFIX_BYTES, PREFIX_BYTES + 1, 512] {
		for header_field in [0, PREFIX_BYTES] {
			for source in [independent.as_slice(), APPEND_SOURCE, APPEND_OUTPUT] {
				assert!(matches!(
					V1AppendPlan::prepare(&wrapped(source, base, header_field), &ADDITIONS),
					Err(V1AppendError::Archive(MpqError::UnsupportedArchive(
						"user data header"
					)))
				));
			}
			assert!(matches!(
				plan.verify(&wrapped(APPEND_OUTPUT, base, header_field)),
				Err(V1AppendError::Archive(MpqError::UnsupportedArchive(
					"user data header"
				)))
			));
		}
	}
}

#[test]
fn the_physical_source_limit_is_checked_before_any_prefix_read() {
	struct Oversized;
	impl Seek for Oversized {
		fn seek(&mut self, seek: SeekFrom) -> io::Result<u64> {
			assert_eq!(seek, SeekFrom::End(0));
			Ok(MAX_ARCHIVE_BYTES + PREFIX_BYTES as u64)
		}
	}
	impl Read for Oversized {
		fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
			panic!("oversized source must not be read");
		}
	}
	assert!(matches!(
		MetadataArchive::open(Oversized),
		Err(MpqError::LimitExceeded(Limit::ArchiveBytes))
	));
}
