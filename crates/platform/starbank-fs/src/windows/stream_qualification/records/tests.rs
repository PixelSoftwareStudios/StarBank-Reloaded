use super::*;

const DEFAULT: &str = "::$DATA";
const LOOKALIKE: &str = ":$DATA:$DATA";

fn record(name: &[u16], size: i64, followed: bool) -> Vec<u8> {
	let name_bytes = std::mem::size_of_val(name);
	let mut bytes = vec![0; NAME_OFFSET];
	write_u32(
		&mut bytes,
		NAME_LENGTH_OFFSET,
		u32::try_from(name_bytes).unwrap(),
	);
	bytes[STREAM_SIZE_OFFSET..STREAM_SIZE_OFFSET + size_of::<i64>()]
		.copy_from_slice(&size.to_le_bytes());
	bytes.extend(name.iter().flat_map(|unit| unit.to_le_bytes()));
	if followed {
		let extent = bytes.len().next_multiple_of(RECORD_ALIGNMENT);
		write_u32(&mut bytes, NEXT_OFFSET, u32::try_from(extent).unwrap());
		bytes.resize(extent, 0);
	}
	bytes
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
	bytes[offset..offset + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}

fn encoded(name: &str) -> Vec<u16> {
	name.encode_utf16().collect()
}

fn assert_invalid(bytes: &[u8]) {
	assert_eq!(parse(bytes).unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn keeps_exact_names_unknown_types_and_an_empty_default_lookalike() {
	let default = encoded(DEFAULT);
	let lookalike = encoded(LOOKALIKE);
	let mut unknown = encoded(":");
	unknown.push(0xd800);
	unknown.extend(":$UNKNOWN".encode_utf16());
	let mut chain = record(&default, 26, true);
	chain.extend(record(&lookalike, 0, true));
	chain.extend(record(&unknown, 5, false));
	assert_eq!(
		parse(&chain).unwrap(),
		[
			Stream {
				name: default,
				size: 26
			},
			Stream {
				name: lookalike,
				size: 0
			},
			Stream {
				name: unknown,
				size: 5
			}
		]
	);
}

#[test]
fn every_strict_prefix_of_an_unpadded_chain_is_incomplete() {
	let mut chain = record(&encoded(DEFAULT), 1, true);
	chain.extend(record(&encoded(":Named:$DATA"), 4, false));
	assert!(parse(&chain).is_ok());
	for length in 0..chain.len() {
		assert_invalid(&chain[..length]);
	}
}

#[test]
fn refuses_nonprogressing_overlapping_misaligned_and_skipped_records() {
	let first = record(&encoded(DEFAULT), 1, true);
	let following = record(&encoded(":Named:$DATA"), 4, false);
	for offset in [1, NAME_OFFSET as u32, u32::MAX, 4096] {
		let mut chain = first.clone();
		chain.extend(&following);
		write_u32(&mut chain, NEXT_OFFSET, offset);
		assert_invalid(&chain);
	}
	let mut skipped = first;
	skipped.extend([0; RECORD_ALIGNMENT]);
	let wrong_offset = u32::try_from(skipped.len()).unwrap();
	skipped.extend(following);
	write_u32(&mut skipped, NEXT_OFFSET, wrong_offset);
	assert_invalid(&skipped);
}

#[test]
fn refuses_invalid_name_lengths_contents_sizes_and_query_extent() {
	let original = record(&encoded(DEFAULT), 1, false);
	for length in [0, 1, u32::MAX - 1] {
		let mut malformed = original.clone();
		write_u32(&mut malformed, NAME_LENGTH_OFFSET, length);
		assert_invalid(&malformed);
	}
	assert_invalid(&record(&[u16::from(b':'), 0], 1, false));
	assert_invalid(&record(&encoded(DEFAULT), -1, false));
	assert_invalid(&vec![0; MAX_BUFFER_BYTES + 1]);
}

#[test]
fn tolerates_only_the_final_record_alignment_padding() {
	let mut last = record(&encoded(DEFAULT), 1, false);
	let unpadded = last.len();
	let aligned = unpadded.next_multiple_of(RECORD_ALIGNMENT);
	assert!(parse(&last).is_ok());
	last.resize(aligned + RECORD_ALIGNMENT, 0xa5);
	assert!(parse(&last[..aligned]).is_ok());
	assert_invalid(&last[..unpadded + 1]);
	assert_invalid(&last);
}
