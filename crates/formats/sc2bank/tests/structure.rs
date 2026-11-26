use sc2bank::{
	LimitKind, Limits, StructuralEdit, StructuralEditError, StructuralNodeId, ValueKind,
	apply_structural_edit, parse,
};

#[test]
fn additions_expand_empty_tags_and_preserve_bom_trivia_and_original_attributes() {
	let source =
		"\u{feff}<?xml version='1.0'?>\r\n<!--before--><Bank version = '1' /><!--after-->\r\n";
	let original = parse(source.as_bytes(), Limits::default()).unwrap();
	let added_section = apply_structural_edit(
		&original,
		StructuralEdit::AddSection { name: "A & B" },
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		added_section.document.source(),
		source
			.replace("/>", "><Section name=\"A &amp; B\"/></Bank>")
			.as_bytes(),
	);
	let section = added_section.document.sections()[0].id();
	assert_eq!(
		added_section.added,
		Some(StructuralNodeId::Section(section))
	);

	let added_key = apply_structural_edit(
		&added_section.document,
		StructuralEdit::AddKey { section, name: "" },
		Limits::default(),
	)
	.unwrap();
	let section_after = &added_key.document.sections()[0];
	assert_eq!(added_key.remap.section(section), Some(section_after.id()));
	let key = section_after.keys()[0].id();
	assert_eq!(added_key.added, Some(StructuralNodeId::Key(key)));
	assert_eq!(section_after.keys()[0].name(), Some(""));

	let added_value = apply_structural_edit(
		&added_key.document,
		StructuralEdit::AddValue {
			key,
			name: "Δelta-1.2",
			kind: &ValueKind::String,
			value: "\"< & >\"\t\r\n",
		},
		Limits::default(),
	)
	.unwrap();
	let section_after = &added_value.document.sections()[0];
	let key_after = &section_after.keys()[0];
	let value_after = &key_after.values()[0];
	assert_eq!(
		added_value.added,
		Some(StructuralNodeId::Value(value_after.id()))
	);
	assert_eq!(added_value.remap.key(key), Some(key_after.id()));
	assert_eq!(value_after.name(), "Δelta-1.2");
	assert_eq!(value_after.attribute().unwrap().value(), "\"< & >\"\t\r\n");
	let expected = "\u{feff}<?xml version='1.0'?>\r\n<!--before--><Bank version = '1' ><Section name=\"A &amp; B\"><Key name=\"\"><Δelta-1.2 string=\"&quot;&lt; &amp; &gt;&quot;&#x9;&#xD;&#xA;\"/></Key></Section></Bank><!--after-->\r\n";
	assert_eq!(added_value.document.source(), expected.as_bytes());
	assert_eq!(original.source(), source.as_bytes());
}

#[test]
fn adding_a_section_keeps_a_trailing_signature_in_place() {
	let source = br#"<Bank><Section name='S'/><!--sig--><Signature value='A'/></Bank>"#;
	let original = parse(source, Limits::default()).unwrap();
	let first = original.sections()[0].id();
	let added = apply_structural_edit(
		&original,
		StructuralEdit::AddSection { name: "S" },
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		added.document.source(),
		br#"<Bank><Section name='S'/><!--sig--><Section name="S"/><Signature value='A'/></Bank>"#,
	);
	assert_eq!(
		added.remap.section(first),
		Some(added.document.sections()[0].id())
	);
	assert_eq!(
		added.added,
		Some(StructuralNodeId::Section(added.document.sections()[1].id()))
	);
	assert_eq!(added.document.signature().unwrap().value(), Some("A"));
}

#[test]
fn deleting_occurrences_excludes_descendants_and_reconciles_later_duplicate_names() {
	let source = concat!(
		"<Bank xmlns:x='urn:keep'>\n",
		"<Section name='S'><Key name='K'><Value int='1'/></Key><x:Nested/></Section>\n",
		"<!--between duplicates--><Section name='S'>",
		"<Key name='K'><Value int='2'/></Key>",
		"<Key name='K'><Value int='3'/><Value int='4'/></Key>",
		"</Section><Signature value='unchanged'/></Bank>",
	);
	let original = parse(source.as_bytes(), Limits::default()).unwrap();
	let first = &original.sections()[0];
	let kept = &original.sections()[1];
	let kept_key = kept.keys()[1].id();
	let kept_value = kept.keys()[1].values()[1].id();
	let result = apply_structural_edit(
		&original,
		StructuralEdit::RemoveSection { id: first.id() },
		Limits::default(),
	)
	.unwrap();

	assert_eq!(
		result.document.source(),
		source
			.replace(
				"<Section name='S'><Key name='K'><Value int='1'/></Key><x:Nested/></Section>",
				""
			)
			.as_bytes(),
	);
	assert!(result.added.is_none());
	assert_eq!(result.remap.section(first.id()), None);
	assert_eq!(result.remap.key(first.keys()[0].id()), None);
	assert_eq!(result.remap.value(first.keys()[0].values()[0].id()), None);
	let current_section = &result.document.sections()[0];
	assert_ne!(current_section.id(), kept.id());
	assert_eq!(result.remap.section(kept.id()), Some(current_section.id()));
	assert_eq!(
		result.remap.key(kept_key),
		Some(current_section.keys()[1].id())
	);
	assert_eq!(
		result.remap.value(kept_value),
		Some(current_section.keys()[1].values()[1].id())
	);

	let removed_key = &current_section.keys()[0];
	let key_result = apply_structural_edit(
		&result.document,
		StructuralEdit::RemoveKey {
			id: removed_key.id(),
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(key_result.remap.key(removed_key.id()), None);
	assert_eq!(key_result.remap.value(removed_key.values()[0].id()), None);
	let remaining_values = key_result.document.sections()[0].keys()[0].values();
	let value_result = apply_structural_edit(
		&key_result.document,
		StructuralEdit::RemoveValue {
			id: remaining_values[0].id(),
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(value_result.remap.value(remaining_values[0].id()), None);
	let only_value = &value_result.document.sections()[0].keys()[0].values()[0];
	assert_eq!(
		value_result.remap.value(remaining_values[1].id()),
		Some(only_value.id())
	);
	assert_eq!(
		value_result.document.source(),
		b"<Bank xmlns:x='urn:keep'>\n\n<!--between duplicates--><Section name='S'><Key name='K'><Value int='4'/></Key></Section><Signature value='unchanged'/></Bank>",
	);
}

#[test]
fn section_and_key_renames_preserve_namespace_and_name_lexemes() {
	let source = br#"<Bank xmlns:x='urn:labels'><Section name  =  'A &#x26; B' x:name='extension'><Key x:name='leave' /></Section></Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	let section = &bank.sections()[0];
	let same = apply_structural_edit(
		&bank,
		StructuralEdit::RenameSection {
			id: section.id(),
			name: "A & B",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(same.document.source(), source);

	let renamed = apply_structural_edit(
		&bank,
		StructuralEdit::RenameSection {
			id: section.id(),
			name: "'quoted' &\tend",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		renamed.document.source(),
		br#"<Bank xmlns:x='urn:labels'><Section name  =  '&apos;quoted&apos; &amp;&#x9;end' x:name='extension'><Key x:name='leave' /></Section></Bank>"#,
	);
	let unnamed = &renamed.document.sections()[0].keys()[0];
	assert!(unnamed.name().is_none());
	let named = apply_structural_edit(
		&renamed.document,
		StructuralEdit::RenameKey {
			id: unnamed.id(),
			name: "Name \"2\"",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		named.document.source(),
		br#"<Bank xmlns:x='urn:labels'><Section name  =  '&apos;quoted&apos; &amp;&#x9;end' x:name='extension'><Key name="Name &quot;2&quot;" x:name='leave' /></Section></Bank>"#,
	);
	assert_eq!(
		named.document.sections()[0].keys()[0].name(),
		Some("Name \"2\"")
	);
}

#[test]
fn value_rename_edits_only_open_and_close_names() {
	let source = br#"<Bank><Section><Key><Old string='raw'><!-- </Old> stays --></Old  ><Old/></Key></Section></Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	let values = bank.sections()[0].keys()[0].values();
	let renamed = apply_structural_edit(
		&bank,
		StructuralEdit::RenameValue {
			id: values[0].id(),
			name: "New",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		renamed.document.source(),
		br#"<Bank><Section><Key><New string='raw'><!-- </Old> stays --></New  ><Old/></Key></Section></Bank>"#,
	);
	let after = renamed.document.sections()[0].keys()[0].values();
	assert_eq!(renamed.remap.value(values[0].id()), Some(after[0].id()));
	assert_eq!(after[0].name(), "New");
	let renamed_empty = apply_structural_edit(
		&renamed.document,
		StructuralEdit::RenameValue {
			id: after[1].id(),
			name: "Empty",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		renamed_empty.document.source(),
		br#"<Bank><Section><Key><New string='raw'><!-- </Old> stays --></New  ><Empty/></Key></Section></Bank>"#,
	);
}

#[test]
fn type_conversion_preserves_equal_value_lexemes_and_updates_an_explicit_replacement() {
	let source = br#"<Bank><Section><Key><Value string  = '&#x31;'/><Value int='3'/></Key></Section></Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	let id = bank.sections()[0].keys()[0].values()[0].id();
	let integer = apply_structural_edit(
		&bank,
		StructuralEdit::SetValueType {
			id,
			kind: &ValueKind::Int,
			value: "1",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		integer.document.source(),
		br#"<Bank><Section><Key><Value int  = '&#x31;'/><Value int='3'/></Key></Section></Bank>"#,
	);
	let fixed = apply_structural_edit(
		&integer.document,
		StructuralEdit::SetValueType {
			id: integer.remap.value(id).unwrap(),
			kind: &ValueKind::Fixed,
			value: "-1.000",
		},
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		fixed.document.source(),
		br#"<Bank><Section><Key><Value fixed  = '-1.000'/><Value int='3'/></Key></Section></Bank>"#,
	);
	let value = fixed.document.sections()[0].keys()[0].values()[0]
		.attribute()
		.unwrap();
	assert_eq!(value.kind(), &ValueKind::Fixed);
	assert_eq!(value.value(), "-1.000");
}

#[test]
fn invalid_or_ambiguous_operations_leave_the_source_unchanged() {
	let source = br#"<Bank xmlns:x='urn:other'><Section><Key><x:Value string='x'/><Value int='1' fixed='0'/><Value future='raw'/></Key></Section></Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	let key = &bank.sections()[0].keys()[0];
	for name in ["1start", "x:Value", "", "Value/><Other"] {
		let edit = StructuralEdit::AddValue {
			key: key.id(),
			name,
			kind: &ValueKind::String,
			value: "ok",
		};
		assert!(matches!(
			apply_structural_edit(&bank, edit, Limits::default()),
			Err(StructuralEditError::InvalidElementName),
		));
	}
	for node in &key.values()[..2] {
		assert!(matches!(
			apply_structural_edit(
				&bank, StructuralEdit::RenameValue { id: node.id(), name: "New" }, Limits::default()
			),
			Err(StructuralEditError::UnsupportedTarget(StructuralNodeId::Value(id))) if id == node.id(),
		));
	}
	assert!(matches!(
		apply_structural_edit(
			&bank,
			StructuralEdit::SetValueType {
				id: key.values()[2].id(),
				kind: &ValueKind::String,
				value: "ok"
			},
			Limits::default(),
		),
		Err(StructuralEditError::UnsupportedTarget(
			StructuralNodeId::Value(_)
		)),
	));
	assert!(matches!(
		apply_structural_edit(
			&bank,
			StructuralEdit::AddValue {
				key: key.id(),
				name: "Value",
				kind: &ValueKind::Unknown("future".into()),
				value: "ok"
			},
			Limits::default(),
		),
		Err(StructuralEditError::UnsupportedKind),
	));
	assert!(matches!(
		apply_structural_edit(
			&bank,
			StructuralEdit::AddSection { name: "\u{0}" },
			Limits::default()
		),
		Err(StructuralEditError::InvalidCharacter('\u{0}')),
	));
	let empty = parse(b"<Bank/>", Limits::default()).unwrap();
	assert!(matches!(
		apply_structural_edit(&empty, StructuralEdit::RemoveKey { id: key.id() }, Limits::default()),
		Err(StructuralEditError::UnknownTarget(StructuralNodeId::Key(id))) if id == key.id(),
	));
	assert_eq!(bank.source(), source);
	assert_eq!(empty.source(), b"<Bank/>");
}

#[test]
fn edit_limits_are_enforced_on_the_entire_candidate() {
	let bank = parse(b"<Bank/>", Limits::default()).unwrap();
	let name = StructuralEdit::AddSection { name: "S" };
	for (limits, expected) in [
		(
			Limits {
				document_bytes: 8,
				..Limits::default()
			},
			LimitKind::DocumentBytes,
		),
		(
			Limits {
				element_depth: 1,
				..Limits::default()
			},
			LimitKind::ElementDepth,
		),
		(
			Limits {
				nodes: 1,
				..Limits::default()
			},
			LimitKind::Nodes,
		),
	] {
		assert!(matches!(
			apply_structural_edit(&bank, name, limits),
			Err(StructuralEditError::LimitExceeded(kind)) if kind == expected,
		));
	}
	assert!(matches!(
		apply_structural_edit(
			&bank,
			StructuralEdit::AddSection { name: "&&" },
			Limits {
				scalar_bytes: 6,
				..Limits::default()
			},
		),
		Err(StructuralEditError::LimitExceeded(LimitKind::ScalarBytes)),
	));
	assert_eq!(bank.source(), b"<Bank/>");
}
