#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use sc2bank::{BankDocument, Limits, ValueEdit, ValueKind, ValueNode, parse, serialize};

const MAX_INPUT_BYTES: usize = 4096;
const MAX_REPLACEMENT_INPUT_BYTES: usize = 256;
const LIMITS: Limits = Limits {
	document_bytes: 16 * 1024,
	element_depth: 12,
	nodes: 256,
	attributes_per_element: 16,
	scalar_bytes: 12 * 1024,
	markup_markers: 2048,
};
const EDIT_FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/opaque-utf8-bom.sc2bank");

struct EditBank {
	bytes: Vec<u8>,
	document: BankDocument,
}

static EDIT_BANKS: OnceLock<[EditBank; 3]> = OnceLock::new();

fuzz_target!(|data: &[u8]| {
	if data.len() > MAX_INPUT_BYTES {
		return;
	}

	if let Ok(bank) = parse(data, LIMITS) {
		assert_eq!(bank.source(), data);
		let output = serialize(&bank, &[], LIMITS).expect("an accepted bank can export unchanged");
		assert_eq!(output, data);
	}

	if let Some((&selector, text)) = data.split_first() {
		check_targeted_edit(selector, text);
	}
});

fn check_targeted_edit(selector: u8, text: &[u8]) {
	let banks = EDIT_BANKS.get_or_init(edit_banks);
	let selector = usize::from(selector);
	let EditBank {
		bytes,
		document: bank,
	} = &banks[selector % banks.len()];
	let mut candidates = value_nodes(bank).filter_map(|node| {
		let attribute = node.attribute()?;
		(!matches!(attribute.kind(), ValueKind::Unknown(_))).then_some((node.id(), attribute))
	});
	let candidate_count = candidates.clone().count();
	assert!(
		candidate_count > 0,
		"authored fixture has editable attributes"
	);
	let (id, attribute) = candidates
		.nth((selector / banks.len()) % candidate_count)
		.expect("fixture contains the selected editable attribute");
	let replacement: String = String::from_utf8_lossy(&text[..text.len().min(MAX_REPLACEMENT_INPUT_BYTES)])
		.chars()
		.filter(|ch| {
			matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
		})
		.collect();

	let range = attribute.source_range();
	let output = serialize(
		bank,
		&[ValueEdit {
			id,
			value: &replacement,
		}],
		LIMITS,
	)
	.expect("bounded XML attribute edit serializes");
	assert_eq!(bank.source(), bytes);
	let prefix = &bytes[..range.start];
	let suffix = &bytes[range.end..];
	assert!(output.len() >= prefix.len() + suffix.len());
	assert!(output.starts_with(prefix));
	assert!(output.ends_with(suffix));

	let reparsed = parse(&output, LIMITS).expect("edited bank reparses");
	assert_eq!(reparsed.encoding(), bank.encoding());
	let changed = value_nodes(&reparsed)
		.find(|node| node.id() == id)
		.and_then(ValueNode::attribute)
		.expect("edited attribute retains its value ID");
	assert_eq!(changed.kind(), attribute.kind());
	assert_eq!(changed.value(), replacement);
}

fn edit_banks() -> [EditBank; 3] {
	let utf16 = std::str::from_utf8(EDIT_FIXTURE)
		.expect("authored fixture is UTF-8")
		.replacen("encoding=\"UTF-8\"", "encoding=\"UTF-16\"", 1);
	[
		EDIT_FIXTURE.to_vec(),
		utf16.encode_utf16().flat_map(u16::to_le_bytes).collect(),
		utf16.encode_utf16().flat_map(u16::to_be_bytes).collect(),
	]
	.map(|bytes| {
		let document = parse(&bytes, LIMITS).expect("authored edit fixture parses");
		EditBank { bytes, document }
	})
}

fn value_nodes(bank: &BankDocument) -> impl Iterator<Item = &ValueNode> + Clone {
	bank.sections()
		.iter()
		.flat_map(|section| section.keys())
		.flat_map(|key| key.values())
}
