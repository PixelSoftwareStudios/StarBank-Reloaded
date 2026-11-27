#![no_main]

use std::{ops::Range, sync::OnceLock};

use libfuzzer_sys::fuzz_target;
use starbank_mpq::{V1Addition, V1AdditionIssue, V1AppendError, V1AppendPlan, V1PreservationIssue};
use stormlib_rs::{decrypt_mpq_block, derive_file_key, encrypt_mpq_block};

const MAX_INPUT_BYTES: usize = 4096;
const MAX_NAME_OR_PAYLOAD_BYTES: usize = 128;
const MAX_MUTATIONS: usize = 32;
const MUTATION_BYTES: usize = 3;
const SOURCE: &[u8] = include_bytes!("../../tests/fixtures/append-v1-source.s2ma");
const OUTPUT: &[u8] = include_bytes!("../../tests/fixtures/append-v1-output.s2ma");
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

const HEADER_SIZE_FIELD: usize = 4;
const HASH_OFFSET_FIELD: usize = 16;
const BLOCK_OFFSET_FIELD: usize = 20;
const HASH_COUNT_FIELD: usize = 24;
const BLOCK_COUNT_FIELD: usize = 28;
const TABLE_ENTRY_BYTES: usize = 16;
const HASH_LOCALE: Range<usize> = 8..10;
const HASH_BLOCK_INDEX_FIELD: usize = 12;

static PLAN: OnceLock<V1AppendPlan<'static>> = OnceLock::new();

fuzz_target!(|data: &[u8]| {
	if data.len() > MAX_INPUT_BYTES {
		return;
	}
	let plan = PLAN.get_or_init(|| {
		let plan = V1AppendPlan::prepare(SOURCE, &ADDITIONS).expect("synthetic source preflights");
		plan.verify(OUTPUT).expect("synthetic append verifies");
		plan
	});
	check_source(data);
	if plan.verify(data).is_ok() {
		let retained = retained_payload();
		assert_eq!(data.get(retained.clone()), Some(&SOURCE[retained]));
	}

	let Some((&selector, input)) = data.split_first() else {
		return;
	};
	match selector % 8 {
		0 => {
			let mut candidate = OUTPUT.to_vec();
			mutate(&mut candidate, input);
			let _ = plan.verify(&candidate);
		}
		1 => {
			let mut source = SOURCE.to_vec();
			mutate(&mut source, input);
			check_source(&source);
		}
		2 => check_retained_bytes(plan, input),
		3 => {
			let mut candidate = OUTPUT.to_vec();
			let added = word(SOURCE, HASH_OFFSET_FIELD)..word(OUTPUT, HASH_OFFSET_FIELD);
			flip_byte(&mut candidate[added], input);
			expect_preservation_error(plan, &candidate, V1PreservationIssue::AddedMember);
		}
		4 => check_hash_table(plan, input),
		5 => {
			let mut candidate = OUTPUT.to_vec();
			edit_table(
				&mut candidate,
				BLOCK_OFFSET_FIELD,
				BLOCK_COUNT_FIELD,
				"(block table)",
				|table| {
					let retained = word(SOURCE, BLOCK_COUNT_FIELD) * TABLE_ENTRY_BYTES;
					flip_byte(&mut table[..retained], input);
				},
			);
			assert!(
				plan.verify(&candidate).is_err(),
				"retained block row changed"
			);
		}
		6 => check_names(input),
		_ => check_proposed_payload(input),
	}
});

fn check_source(bytes: &[u8]) {
	if let Ok(plan) = V1AppendPlan::prepare(bytes, &ADDITIONS) {
		assert_eq!(plan.source(), bytes);
		assert!(
			plan.verify(bytes).is_err(),
			"a nonempty append cannot leave the source unchanged"
		);
		let _ = plan.verify(OUTPUT);
	}
}

fn check_retained_bytes(plan: &V1AppendPlan<'_>, input: &[u8]) {
	let retained = retained_payload();
	let mut candidate = OUTPUT.to_vec();
	flip_byte(&mut candidate[retained.clone()], input);
	expect_preservation_error(plan, &candidate, V1PreservationIssue::RetainedPayload);

	let mut source = SOURCE.to_vec();
	source[retained.clone()].copy_from_slice(&candidate[retained]);
	let matching = V1AppendPlan::prepare(&source, &ADDITIONS)
		.expect("one opaque retained payload byte does not change the archive layout");
	matching
		.verify(&candidate)
		.expect("both archives retain the same packed bytes");
	assert!(
		matching.verify(OUTPUT).is_err(),
		"the original candidate lacks the changed byte"
	);
}

fn retained_payload() -> Range<usize> {
	word(SOURCE, HEADER_SIZE_FIELD)..word(SOURCE, HASH_OFFSET_FIELD)
}

fn check_hash_table(plan: &V1AppendPlan<'_>, input: &[u8]) {
	let mut candidate = OUTPUT.to_vec();
	edit_table(
		&mut candidate,
		HASH_OFFSET_FIELD,
		HASH_COUNT_FIELD,
		"(hash table)",
		|table| {
			let original_count = word(SOURCE, BLOCK_COUNT_FIELD);
			let selected = usize::from(input.first().copied().unwrap_or(0)) % original_count;
			let retained = table
				.as_chunks_mut::<TABLE_ENTRY_BYTES>()
				.0
				.iter_mut()
				.filter(|slot| word(slot.as_slice(), HASH_BLOCK_INDEX_FIELD) < original_count)
				.nth(selected)
				.expect("fixture hash table references each retained block");
			flip_byte(
				&mut retained[HASH_LOCALE],
				input.get(1..).unwrap_or_default(),
			);
		},
	);
	expect_preservation_error(plan, &candidate, V1PreservationIssue::RetainedTable);
}

fn check_names(input: &[u8]) {
	let Some((&selector, text)) = input.split_first() else {
		return;
	};
	let text = &text[..text.len().min(MAX_NAME_OR_PAYLOAD_BYTES)];
	match selector % 3 {
		0 => {
			let name: String = ADDITIONS[0]
				.name
				.bytes()
				.enumerate()
				.map(|(index, byte)| {
					if text.get(index).copied().unwrap_or(0) & 1 == 0 {
						char::from(byte.to_ascii_uppercase())
					} else {
						char::from(byte.to_ascii_lowercase())
					}
				})
				.collect();
			let additions = [
				V1Addition {
					name: &name,
					..ADDITIONS[0]
				},
				ADDITIONS[1],
			];
			let plan =
				V1AppendPlan::prepare(SOURCE, &additions).expect("same portable member name");
			plan.verify(OUTPUT)
				.expect("classic member lookup is ASCII case-insensitive");
		}
		1 => {
			let name = format!("Fixture\\..\\{}", String::from_utf8_lossy(text));
			let additions = [
				V1Addition {
					name: &name,
					..ADDITIONS[0]
				},
				ADDITIONS[1],
			];
			assert!(matches!(
				V1AppendPlan::prepare(SOURCE, &additions),
				Err(V1AppendError::Addition(V1AdditionIssue::Name))
			));
		}
		_ => {
			let name = String::from_utf8_lossy(text);
			let additions = [
				V1Addition {
					name: &name,
					..ADDITIONS[0]
				},
				ADDITIONS[1],
			];
			if let Ok(plan) = V1AppendPlan::prepare(SOURCE, &additions) {
				let _ = plan.verify(OUTPUT);
			}
		}
	}
}

fn check_proposed_payload(input: &[u8]) {
	let bytes = &input[..input.len().min(MAX_NAME_OR_PAYLOAD_BYTES)];
	let additions = [
		V1Addition {
			bytes,
			..ADDITIONS[0]
		},
		ADDITIONS[1],
	];
	if bytes.is_empty() {
		assert!(V1AppendPlan::prepare(SOURCE, &additions).is_err());
		return;
	}
	let plan = V1AppendPlan::prepare(SOURCE, &additions).expect("bounded nonempty payload");
	if bytes == ADDITIONS[0].bytes {
		plan.verify(OUTPUT).expect("both added members are present");
	} else {
		expect_preservation_error(&plan, OUTPUT, V1PreservationIssue::AddedMember);
	}
}

fn expect_preservation_error(
	plan: &V1AppendPlan<'_>,
	output: &[u8],
	expected: V1PreservationIssue,
) {
	let actual = plan.verify(output);
	assert!(
		matches!(actual, Err(V1AppendError::Preservation(issue)) if issue == expected),
		"expected {expected:?}, got {actual:?}"
	);
}

fn mutate(bytes: &mut [u8], input: &[u8]) {
	for edit in input
		.as_chunks::<MUTATION_BYTES>()
		.0
		.iter()
		.take(MAX_MUTATIONS)
	{
		flip_byte(bytes, edit);
	}
}

fn flip_byte(bytes: &mut [u8], input: &[u8]) {
	let mut input = input.iter().copied().chain([0; MUTATION_BYTES]);
	let offset = u16::from_le_bytes([input.next().unwrap_or(0), input.next().unwrap_or(0)]);
	let index = usize::from(offset) % bytes.len();
	bytes[index] ^= input.next().unwrap_or(0).max(1);
}

fn edit_table(
	archive: &mut [u8],
	offset: usize,
	count: usize,
	name: &str,
	edit: impl FnOnce(&mut [u8]),
) {
	let start = word(archive, offset);
	let end = start + word(archive, count) * TABLE_ENTRY_BYTES;
	let table = &mut archive[start..end];
	let key = derive_file_key(name, 0, 0, false);
	decrypt_mpq_block(table, key);
	edit(table);
	encrypt_mpq_block(table, key);
}

fn word(bytes: &[u8], offset: usize) -> usize {
	let bytes = bytes[offset..offset + 4]
		.try_into()
		.expect("synthetic fixture header or table row");
	u32::from_le_bytes(bytes) as usize
}
