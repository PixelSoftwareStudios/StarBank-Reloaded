use std::{
	fs::File,
	io::{self, Cursor, Read, Seek, SeekFrom},
	path::PathBuf,
};

use starbank_mpq::{Limit, MetadataArchive, MetadataEntry, MpqError};
use stormlib_rs::{decrypt_mpq_block, derive_file_key, encrypt_mpq_block};

const V1_STORED_AND_ZLIB: &[u8] = include_bytes!("fixtures/v1-no-list.s2ma");
const V1_BZIP2_AND_MISSING: &[u8] = include_bytes!("fixtures/v1-list.s2ma");
const V2_WITH_LISTFILE: &[u8] = include_bytes!("fixtures/v2-list.s2ma");
const V4_WITHOUT_LISTFILE: &[u8] = include_bytes!("fixtures/v4-no-list.s2ma");
const ENTRIES: [MetadataEntry; 3] = [
	MetadataEntry::MapScript,
	MetadataEntry::BaseMapScript,
	MetadataEntry::DocumentHeader,
];

fn payload() -> Vec<u8> {
	let mut text = String::from("// Name: Synthetic – alpha\n// Author: Fixture only\n");
	text.push_str(&"// Content for sector coverage.\n".repeat(320));
	text.into_bytes()
}

#[test]
fn stormlib_archive_entries_match_the_original_bytes() {
	let expected = payload();
	for fixture in [V1_STORED_AND_ZLIB, V2_WITH_LISTFILE, V4_WITHOUT_LISTFILE] {
		let mut archive = MetadataArchive::open(Cursor::new(fixture)).unwrap();
		for entry in ENTRIES {
			assert_eq!(
				archive.read(entry).unwrap().as_deref(),
				Some(expected.as_slice())
			);
		}
	}
}

#[test]
fn other_compression_and_missing_entries_have_distinct_outcomes() {
	let mut archive = MetadataArchive::open(Cursor::new(V1_BZIP2_AND_MISSING)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::MapScript),
		Err(MpqError::UnsupportedEntry(MetadataEntry::MapScript))
	));
	assert_eq!(archive.read(MetadataEntry::DocumentHeader).unwrap(), None);
	assert_eq!(
		archive.read(MetadataEntry::BaseMapScript).unwrap(),
		Some(payload())
	);
}

#[test]
fn validates_header_and_table_counts_before_table_reads() {
	assert!(matches!(
		MetadataArchive::open(Cursor::new(&V1_STORED_AND_ZLIB[..12])),
		Err(MpqError::InvalidArchive(_))
	));
	assert!(matches!(
		MetadataArchive::open(Cursor::new(
			&V1_STORED_AND_ZLIB[..V1_STORED_AND_ZLIB.len() - 1]
		)),
		Err(MpqError::InvalidArchive(_))
	));

	let mut fixture = V1_STORED_AND_ZLIB.to_vec();
	fixture[24..28].copy_from_slice(&(1u32 << 17).to_le_bytes());
	assert!(matches!(
		MetadataArchive::open(Cursor::new(fixture)),
		Err(MpqError::LimitExceeded(Limit::TableEntries))
	));
}

#[test]
fn rejects_unknown_header_and_compressed_classic_tables() {
	let mut fixture = V1_STORED_AND_ZLIB.to_vec();
	fixture[12..14].copy_from_slice(&u16::MAX.to_le_bytes());
	assert!(matches!(
		MetadataArchive::open(Cursor::new(fixture)),
		Err(MpqError::UnsupportedArchive(_))
	));
	let mut fixture = V4_WITHOUT_LISTFILE.to_vec();
	fixture[68..76].copy_from_slice(&1u64.to_le_bytes());
	assert!(matches!(
		MetadataArchive::open(Cursor::new(fixture)),
		Err(MpqError::UnsupportedArchive(_))
	));
}

#[test]
fn rejects_block_table_references_outside_the_declared_count() {
	let mut fixture = V1_STORED_AND_ZLIB.to_vec();
	let table_offset = field(&fixture[16..20]);
	let table_count = field(&fixture[24..28]);
	let table = &mut fixture[table_offset..table_offset + table_count * 16];
	let key = derive_file_key("(hash table)", 0, 0, false);
	decrypt_mpq_block(table, key);
	table[12..16].copy_from_slice(&(table_count as u32).to_le_bytes());
	encrypt_mpq_block(table, key);
	assert!(matches!(
		MetadataArchive::open(Cursor::new(fixture)),
		Err(MpqError::InvalidArchive(_))
	));
}

#[test]
fn enforces_entry_size_and_extent() {
	let fixture = edit_blocks(|entry| entry[8..12].copy_from_slice(&(17u32 << 20).to_le_bytes()));
	let mut archive = MetadataArchive::open(Cursor::new(fixture)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::MapScript),
		Err(MpqError::LimitExceeded(Limit::EntryBytes))
	));

	let fixture = edit_blocks(|entry| entry[..4].copy_from_slice(&0u32.to_le_bytes()));
	let mut archive = MetadataArchive::open(Cursor::new(fixture)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::MapScript),
		Err(MpqError::InvalidEntry(MetadataEntry::MapScript))
	));
}

#[test]
fn rejects_expanded_size_mismatch_and_invalid_sector_range() {
	let fixture = edit_blocks(|entry| {
		let size = field(&entry[8..12]) as u32;
		entry[8..12].copy_from_slice(&(size - 1).to_le_bytes());
	});
	let mut archive = MetadataArchive::open(Cursor::new(fixture)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::BaseMapScript),
		Err(MpqError::InvalidEntry(MetadataEntry::BaseMapScript))
	));

	let mut fixture = V1_STORED_AND_ZLIB.to_vec();
	let first_sector_offset = 32..36;
	fixture[first_sector_offset].copy_from_slice(&u32::MAX.to_le_bytes());
	let mut archive = MetadataArchive::open(Cursor::new(fixture)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::MapScript),
		Err(MpqError::InvalidEntry(MetadataEntry::MapScript))
	));
}

#[test]
fn enforces_sector_count_before_reading_entry_data() {
	let mut fixture =
		edit_blocks(|entry| entry[8..12].copy_from_slice(&(3u32 << 20).to_le_bytes()));
	fixture[14..16].copy_from_slice(&0u16.to_le_bytes());
	let mut archive = MetadataArchive::open(Cursor::new(fixture)).unwrap();
	assert!(matches!(
		archive.read(MetadataEntry::MapScript),
		Err(MpqError::LimitExceeded(Limit::EntrySectors))
	));
}

#[test]
fn reads_from_a_native_read_only_file_handle() {
	let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v4-no-list.s2ma");
	let mut archive = MetadataArchive::open(File::open(path).unwrap()).unwrap();
	assert_eq!(
		archive.read(MetadataEntry::MapScript).unwrap(),
		Some(payload())
	);
}

#[test]
fn large_sources_are_rejected_without_a_read() {
	#[derive(Debug)]
	struct LargeSource;

	impl Seek for LargeSource {
		fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
			assert_eq!(target, SeekFrom::End(0));
			Ok(513 << 20)
		}
	}

	impl Read for LargeSource {
		fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
			panic!("archive limit must be checked before any read");
		}
	}

	assert!(matches!(
		MetadataArchive::open(LargeSource),
		Err(MpqError::LimitExceeded(Limit::ArchiveBytes))
	));
}

fn field(bytes: &[u8]) -> usize {
	u32::from_le_bytes(bytes.try_into().unwrap()) as usize
}

fn edit_blocks(mut edit: impl FnMut(&mut [u8])) -> Vec<u8> {
	let mut fixture = V1_STORED_AND_ZLIB.to_vec();
	let block_offset = field(&fixture[20..24]);
	let block_count = field(&fixture[28..32]);
	let table = &mut fixture[block_offset..block_offset + block_count * 16];
	let key = derive_file_key("(block table)", 0, 0, false);
	decrypt_mpq_block(table, key);
	for entry in table.as_chunks_mut::<16>().0 {
		edit(entry);
	}
	encrypt_mpq_block(table, key);
	fixture
}
