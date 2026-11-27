use std::{error::Error, io::Cursor};

use starbank_mpq::{
	MetadataArchive, MetadataEntry, V1Addition, V1AdditionIssue, V1AppendError, V1AppendPlan,
	V1LayoutIssue, V1PreservationIssue,
};
use stormlib_rs::{decrypt_mpq_block, derive_file_key, encrypt_mpq_block};

const SOURCE: &[u8] = include_bytes!("fixtures/append-v1-source.s2ma");
const OUTPUT: &[u8] = include_bytes!("fixtures/append-v1-output.s2ma");
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
const RECORD_BYTES: usize = 16;

#[test]
fn verifies_exact_stormlib_append_with_an_opaque_packed_member() -> Result<(), Box<dyn Error>> {
	let plan = V1AppendPlan::prepare(SOURCE, &ADDITIONS)?;
	assert_eq!(plan.source(), SOURCE);
	assert_eq!(plan.additions().len(), ADDITIONS.len());
	assert_eq!(plan.additions()[0].name, ADDITIONS[0].name);
	assert_eq!(plan.additions()[1].bytes, ADDITIONS[1].bytes);
	assert_eq!(
		format!("{plan:?}"),
		format!(
			"V1AppendPlan {{ source_bytes: {}, addition_count: {}, .. }}",
			SOURCE.len(),
			ADDITIONS.len()
		)
	);
	plan.verify(OUTPUT)?;

	let mut before = MetadataArchive::open(Cursor::new(SOURCE))?;
	let mut after = MetadataArchive::open(Cursor::new(OUTPUT))?;
	assert_eq!(
		before.read(MetadataEntry::MapScript)?,
		after.read(MetadataEntry::MapScript)?
	);
	assert_ne!(SOURCE, OUTPUT);
	Ok(())
}

#[test]
fn preflight_refuses_other_versions_trailers_and_managed_internal_members() {
	let mut trailer = SOURCE.to_vec();
	trailer.extend_from_slice(b"NGIS opaque trailer");
	for (archive, issue) in [
		(
			include_bytes!("fixtures/v2-list.s2ma").as_slice(),
			V1LayoutIssue::Version,
		),
		(
			include_bytes!("fixtures/v4-no-list.s2ma").as_slice(),
			V1LayoutIssue::Version,
		),
		(
			include_bytes!("fixtures/v1-list.s2ma").as_slice(),
			V1LayoutIssue::InternalMember,
		),
		(trailer.as_slice(), V1LayoutIssue::Trailer),
	] {
		let refusal = V1AppendPlan::prepare(archive, &ADDITIONS).err();
		assert!(
			matches!(refusal, Some(V1AppendError::Layout(actual)) if actual == issue),
			"expected {issue:?}, got {refusal:?}"
		);
	}
}

#[test]
fn preflight_refuses_unsafe_duplicate_and_existing_names_and_table_growth() {
	assert!(matches!(
		V1AppendPlan::prepare(SOURCE, &[]),
		Err(V1AppendError::Addition(V1AdditionIssue::Count))
	));
	for name in [
		"",
		"(attributes)",
		"File00000001.xxx",
		"fIlE00000042.any.extension",
		r"FILE00000042.dir\Child",
		r"Fixture\..\Restored.txt",
		"Fixture/Restored.txt",
		r"C:\Outside.txt",
		r"Fixture\NUL.txt",
		"Fixture\\",
		"Win COM1.txt.",
		"Ñ.txt",
	] {
		let proposed = [V1Addition { name, bytes: b"x" }];
		let plan = V1AppendPlan::prepare(SOURCE, &proposed);
		assert!(
			matches!(plan, Err(V1AppendError::Addition(V1AdditionIssue::Name))),
			"{name}"
		);
	}
	for name in [
		"FILE0000000A.bin",
		"FILE00000042",
		r"Fixture\File00000042.bin",
	] {
		let addition = [V1Addition { name, bytes: b"x" }];
		assert!(V1AppendPlan::prepare(SOURCE, &addition).is_ok(), "{name}");
	}

	let rival = V1Addition {
		name: "fixture\\RESTORED.txt",
		bytes: b"other",
	};
	assert!(matches!(
		V1AppendPlan::prepare(SOURCE, &[ADDITIONS[0], rival]),
		Err(V1AppendError::Addition(V1AdditionIssue::Name))
	));
	assert!(matches!(
		V1AppendPlan::prepare(
			SOURCE,
			&[V1Addition {
				name: "Extra.txt",
				bytes: b""
			}]
		),
		Err(V1AppendError::Addition(V1AdditionIssue::ComponentSize))
	));

	let mut localized = SOURCE.to_vec();
	edit_table(&mut localized, Table::Hash, |table| {
		let script = table
			.as_chunks_mut::<RECORD_BYTES>()
			.0
			.iter_mut()
			.find(|slot| word(&slot[..], 12) == 0)
			.expect("known source block");
		script[8..10].copy_from_slice(&0x0409_u16.to_le_bytes());
	});
	assert!(matches!(
		V1AppendPlan::prepare(
			&localized,
			&[V1Addition {
				name: "mapscript.GALAXY",
				bytes: b"x"
			}]
		),
		Err(V1AppendError::Addition(V1AdditionIssue::ExistingMember))
	));

	let names: Vec<_> = (0..16).map(|index| format!("entry-{index}.txt")).collect();
	let additions: Vec<_> = names
		.iter()
		.map(|name| V1Addition { name, bytes: b"x" })
		.collect();
	assert!(matches!(
		V1AppendPlan::prepare(SOURCE, &additions),
		Err(V1AppendError::Addition(V1AdditionIssue::Capacity))
	));
}

#[test]
fn preflight_refuses_reserved_flags_and_invalid_packed_references() {
	let mut reserved = SOURCE.to_vec();
	edit_table(&mut reserved, Table::Hash, |table| {
		let slot = table
			.as_chunks_mut::<RECORD_BYTES>()
			.0
			.iter_mut()
			.find(|slot| word(&slot[..], 12) == 0)
			.expect("known source block");
		slot[11] = 1;
	});
	assert!(matches!(
		V1AppendPlan::prepare(&reserved, &ADDITIONS),
		Err(V1AppendError::Layout(V1LayoutIssue::Flags))
	));

	let mut patch_flag = SOURCE.to_vec();
	edit_table(&mut patch_flag, Table::Block, |table| {
		let flags = word(table, 12) | 0x0010_0000;
		table[12..16].copy_from_slice(&flags.to_le_bytes());
	});
	assert!(matches!(
		V1AppendPlan::prepare(&patch_flag, &ADDITIONS),
		Err(V1AppendError::Layout(V1LayoutIssue::Flags))
	));

	let mut duplicate = SOURCE.to_vec();
	edit_table(&mut duplicate, Table::Hash, |table| {
		let slot = table
			.as_chunks_mut::<RECORD_BYTES>()
			.0
			.iter_mut()
			.find(|slot| word(&slot[..], 12) == 1)
			.expect("known source block");
		slot[12..16].copy_from_slice(&0_u32.to_le_bytes());
	});
	assert!(matches!(
		V1AppendPlan::prepare(&duplicate, &ADDITIONS),
		Err(V1AppendError::Layout(V1LayoutIssue::BlockReferences))
	));

	let mut overlap = SOURCE.to_vec();
	edit_table(&mut overlap, Table::Block, |table| {
		let first = word(table, 0);
		table[16..20].copy_from_slice(&first.to_le_bytes());
	});
	assert!(matches!(
		V1AppendPlan::prepare(&overlap, &ADDITIONS),
		Err(V1AppendError::Layout(V1LayoutIssue::PayloadRanges))
	));
}

#[test]
fn verifies_retained_hash_slots_block_metadata_and_every_packed_byte() -> Result<(), Box<dyn Error>>
{
	let plan = V1AppendPlan::prepare(SOURCE, &ADDITIONS)?;
	let mut changed_hash = OUTPUT.to_vec();
	edit_table(&mut changed_hash, Table::Hash, |table| {
		let slot = table
			.as_chunks_mut::<RECORD_BYTES>()
			.0
			.iter_mut()
			.find(|slot| word(&slot[..], 12) == 0)
			.expect("known source block");
		slot[8..10].copy_from_slice(&0x0409_u16.to_le_bytes());
	});
	assert!(matches!(
		plan.verify(&changed_hash),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::RetainedTable
		))
	));

	let mut changed_block = OUTPUT.to_vec();
	edit_table(&mut changed_block, Table::Block, |table| {
		// A stored single-unit entry remains structurally valid with the same packed bytes.
		let flags = word(table, 12) | 0x0100_0000;
		table[12..16].copy_from_slice(&flags.to_le_bytes());
	});
	assert!(matches!(
		plan.verify(&changed_block),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::RetainedTable
		))
	));

	let mut changed_packed_bytes = OUTPUT.to_vec();
	let mut offset = None;
	edit_table(&mut changed_packed_bytes, Table::Block, |table| {
		offset = Some(word(table, 16) as usize);
	});
	changed_packed_bytes[offset.expect("packed member offset") + 19] ^= 1;
	assert!(matches!(
		plan.verify(&changed_packed_bytes),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::RetainedPayload
		))
	));
	Ok(())
}

#[test]
fn requires_all_and_only_intended_added_names_and_payloads() -> Result<(), Box<dyn Error>> {
	let plan = V1AppendPlan::prepare(SOURCE, &ADDITIONS)?;
	assert!(matches!(
		plan.verify(SOURCE),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::RetainedTable
		))
	));
	let single_addition = V1AppendPlan::prepare(SOURCE, &ADDITIONS[..1])?;
	assert!(matches!(
		single_addition.verify(OUTPUT),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::RetainedTable
		))
	));
	let mut changed = OUTPUT.to_vec();
	let first_added_byte = word(SOURCE, 16) as usize;
	changed[first_added_byte] ^= 1;
	assert!(matches!(
		plan.verify(&changed),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::AddedMember
		))
	));

	let wrong = [
		V1Addition {
			name: "Fixture\\Different.txt",
			..ADDITIONS[0]
		},
		ADDITIONS[1],
	];
	let plan = V1AppendPlan::prepare(SOURCE, &wrong)?;
	assert!(matches!(
		plan.verify(OUTPUT),
		Err(V1AppendError::Preservation(
			V1PreservationIssue::AddedMember
		))
	));
	Ok(())
}

#[test]
fn rejects_changed_added_hash_slots_and_block_records() -> Result<(), Box<dyn Error>> {
	const HASH_METADATA: usize = 8;
	const HASH_BLOCK_INDEX: usize = 12;
	const BLOCK_OFFSET: usize = 0;
	const BLOCK_PACKED_BYTES: usize = 4;
	const BLOCK_FILE_BYTES: usize = 8;
	const BLOCK_FLAGS: usize = 12;
	let retained_count = word(SOURCE, 28);
	let plan = V1AppendPlan::prepare(SOURCE, &ADDITIONS)?;
	plan.verify(OUTPUT)?;
	for (field, replacement) in [
		(HASH_METADATA, 0x0409),                // Nonneutral locale.
		(HASH_METADATA, 0x0001_0000),           // Nonneutral platform.
		(HASH_METADATA, 0x0100_0000),           // Reserved hash flags.
		(HASH_BLOCK_INDEX, 0),                  // Alias a retained member.
		(HASH_BLOCK_INDEX, retained_count + 1), // Alias the other addition.
		(HASH_BLOCK_INDEX, retained_count + 2), // Outside the block table.
	] {
		let mut changed = OUTPUT.to_vec();
		edit_table(&mut changed, Table::Hash, |table| {
			let slot = table
				.as_chunks_mut::<RECORD_BYTES>()
				.0
				.iter_mut()
				.find(|slot| word(&slot[..], HASH_BLOCK_INDEX) == retained_count)
				.expect("first added member hash slot");
			slot[field..field + 4].copy_from_slice(&replacement.to_le_bytes());
		});
		assert!(
			plan.verify(&changed).is_err(),
			"hash field {field}, value {replacement}"
		);
	}
	for field in [
		BLOCK_OFFSET,
		BLOCK_PACKED_BYTES,
		BLOCK_FILE_BYTES,
		BLOCK_FLAGS,
	] {
		let mut changed = OUTPUT.to_vec();
		edit_table(&mut changed, Table::Block, |table| {
			let offset = retained_count as usize * RECORD_BYTES + field;
			let replacement = word(table, offset) ^ 1;
			table[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
		});
		assert!(plan.verify(&changed).is_err(), "added block field {field}");
	}
	Ok(())
}

#[test]
fn finds_existing_localized_names_beyond_an_empty_lookup_slot() -> Result<(), Box<dyn Error>> {
	const BLOCK_INDEX_FIELD: usize = 12;
	let mut hidden = SOURCE.to_vec();
	edit_table(&mut hidden, Table::Hash, |table| {
		let slots = table.as_chunks_mut::<RECORD_BYTES>().0;
		let original = slots
			.iter()
			.position(|slot| word(&slot[..], BLOCK_INDEX_FIELD) == 0)
			.expect("original script hash slot");
		let relocated = (1..slots.len())
			.map(|step| (original + step) % slots.len())
			.find(|index| word(&slots[*index][..], BLOCK_INDEX_FIELD) == u32::MAX)
			.expect("empty hash slot after the original");
		slots[relocated] = slots[original];
		slots[relocated][8..10].copy_from_slice(&0x0409_u16.to_le_bytes());
		slots[original].fill(u8::MAX);
	});
	let mut ordinary = MetadataArchive::open(Cursor::new(&hidden))?;
	assert!(ordinary.read(MetadataEntry::MapScript)?.is_none());
	let additions = [V1Addition {
		name: "mapscript.GALAXY",
		bytes: b"x",
	}];
	assert!(matches!(
		V1AppendPlan::prepare(&hidden, &additions),
		Err(V1AppendError::Addition(V1AdditionIssue::ExistingMember))
	));
	Ok(())
}

enum Table {
	Hash,
	Block,
}

fn edit_table(archive: &mut [u8], table: Table, edit: impl FnOnce(&mut [u8])) {
	let (offset_field, count_field, name) = match table {
		Table::Hash => (16, 24, "(hash table)"),
		Table::Block => (20, 28, "(block table)"),
	};
	let start = word(archive, offset_field) as usize;
	let end = start + word(archive, count_field) as usize * RECORD_BYTES;
	let bytes = &mut archive[start..end];
	let key = derive_file_key(name, 0, 0, false);
	decrypt_mpq_block(bytes, key);
	edit(bytes);
	encrypt_mpq_block(bytes, key);
}

fn word(bytes: &[u8], offset: usize) -> u32 {
	u32::from_le_bytes(
		bytes[offset..offset + 4]
			.try_into()
			.expect("test fixture record"),
	)
}
