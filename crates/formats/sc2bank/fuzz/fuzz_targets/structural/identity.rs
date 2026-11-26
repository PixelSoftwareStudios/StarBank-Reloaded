use std::ops::Range;

use sc2bank::{BankDocument, StructuralEdit, StructuralEditResult, StructuralNodeId};

pub(super) fn check_addition(edit: StructuralEdit<'_>, result: &StructuralEditResult) {
	match (edit, result.added) {
		(StructuralEdit::AddSection { name }, Some(StructuralNodeId::Section(id))) => {
			let added = result
				.document
				.sections()
				.iter()
				.find(|section| section.id() == id)
				.expect("new section exists");
			assert_eq!(added.name(), Some(name));
		}
		(StructuralEdit::AddKey { section, name }, Some(StructuralNodeId::Key(id))) => {
			let parent = result
				.remap
				.section(section)
				.expect("added key's parent survives");
			let parent = result
				.document
				.sections()
				.iter()
				.find(|section| section.id() == parent)
				.expect("new key belongs to requested section");
			let added = parent
				.keys()
				.iter()
				.find(|key| key.id() == id)
				.expect("new key exists");
			assert_eq!(added.name(), Some(name));
		}
		(
			StructuralEdit::AddValue {
				key,
				name,
				kind,
				value,
			},
			Some(StructuralNodeId::Value(id)),
		) => {
			let parent = result
				.remap
				.key(key)
				.expect("added value's parent survives");
			let parent = result
				.document
				.sections()
				.iter()
				.flat_map(|section| section.keys())
				.find(|key| key.id() == parent)
				.expect("new value belongs to requested key");
			let added = parent
				.values()
				.iter()
				.find(|node| node.id() == id)
				.expect("new value exists");
			assert_eq!(added.name(), name);
			let attribute = added.attribute().expect("added value remains typed");
			assert_eq!((attribute.kind(), attribute.value()), (kind, value));
		}
		(
			StructuralEdit::AddSection { .. }
			| StructuralEdit::AddKey { .. }
			| StructuralEdit::AddValue { .. },
			_,
		) => {
			panic!("insertion did not report the new occurrence of the expected kind");
		}
		(_, added) => assert_eq!(added, None),
	}
}

fn nodes(bank: &BankDocument) -> Vec<(StructuralNodeId, usize)> {
	let mut nodes = Vec::new();
	for section in bank.sections() {
		nodes.push((
			StructuralNodeId::Section(section.id()),
			section.source_range().start,
		));
		for key in section.keys() {
			nodes.push((StructuralNodeId::Key(key.id()), key.source_range().start));
			for value in key.values() {
				nodes.push((
					StructuralNodeId::Value(value.id()),
					value.source_range().start,
				));
			}
		}
	}
	nodes
}

pub(super) fn check_remap(
	bank: &BankDocument,
	result: &StructuralEditResult,
	removed: Option<Range<usize>>,
) {
	let mut survivors = Vec::new();
	for (original, start) in nodes(bank) {
		let mapped = match original {
			StructuralNodeId::Section(id) => {
				result.remap.section(id).map(StructuralNodeId::Section)
			}
			StructuralNodeId::Key(id) => result.remap.key(id).map(StructuralNodeId::Key),
			StructuralNodeId::Value(id) => result.remap.value(id).map(StructuralNodeId::Value),
			_ => unreachable!("authored fixture contains only section, key and value projections"),
		};
		if removed.as_ref().is_some_and(|range| range.contains(&start)) {
			assert_eq!(
				mapped, None,
				"deleted occurrences and descendants leave the map"
			);
		} else {
			survivors.push(mapped.expect("surviving occurrence remains addressable"));
		}
	}
	let current: Vec<_> = nodes(&result.document)
		.into_iter()
		.map(|(id, _)| id)
		.filter(|&id| Some(id) != result.added)
		.collect();
	assert_eq!(
		survivors, current,
		"remap preserves relative order and distinct duplicate IDs"
	);
}
