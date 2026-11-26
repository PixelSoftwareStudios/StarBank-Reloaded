use sc2bank::{
	LimitKind, Limits, OpaqueReason, ParseErrorKind, SerializeError, ValueEdit, ValueKind, parse,
	serialize,
};

const ORDINARY: &[u8] = include_bytes!("fixtures/ordinary.sc2bank");
const OPAQUE: &[u8] = include_bytes!("fixtures/opaque-utf8-bom.sc2bank");
const AMBIGUOUS: &[u8] = include_bytes!("fixtures/ambiguous-type.sc2bank");
const WHITESPACE: &[u8] = include_bytes!("fixtures/attribute-whitespace.sc2bank");

#[test]
fn ordinary_bank_keeps_exact_numbers_and_xml_decoded_strings() {
	let bank = parse(ORDINARY, Limits::default()).unwrap();
	assert_eq!(bank.version(), Some("1"));
	assert!(bank.signature().is_none());
	assert_eq!(bank.sections()[0].name(), Some("Fixture"));
	let keys = bank.sections()[0].keys();
	let expected = [
		("Count", ValueKind::Int, "9007199254740993"),
		("Rate", ValueKind::Fixed, "-0.1250"),
		("Label", ValueKind::String, "A & B"),
	];
	assert_eq!(keys.len(), expected.len());
	for (key, (name, kind, value)) in keys.iter().zip(expected) {
		assert_eq!(key.name(), Some(name));
		let attribute = key.values()[0].attribute().unwrap();
		assert_eq!(attribute.kind(), &kind);
		assert_eq!(attribute.value(), value);
	}
	assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), ORDINARY);
}

#[test]
fn preserves_opaque_xml_and_all_occurrences_in_source_order() {
	let bank = parse(OPAQUE, Limits::default()).unwrap();
	assert_eq!(bank.source(), OPAQUE);
	assert_eq!(bank.version(), Some("01"));
	assert!(bank.has_unprojected_bank_nodes());

	let sections = bank.sections();
	assert_eq!(sections.len(), 2);
	assert_eq!(sections[0].name(), sections[1].name());
	assert_ne!(sections[0].id(), sections[1].id());

	let keys = sections[0].keys();
	assert_eq!(keys.len(), 3);
	assert_eq!(keys[0].name(), keys[1].name());
	assert_ne!(keys[0].id(), keys[1].id());
	let children = keys[1].values();
	assert_eq!(children.len(), 2);
	assert_eq!(children[0].name(), "Nested");
	assert_eq!(children[1].name(), "Unknown");
	assert_ne!(children[0].id(), children[1].id());
	assert_eq!(children[0].attribute().unwrap().value(), "élan & A");
	let unknown = children[1].attribute().unwrap();
	assert_eq!(unknown.kind(), &ValueKind::Unknown("future".to_owned()));
	assert_eq!(unknown.value(), "a < b");
	assert!(keys[2].values().is_empty());
	assert_eq!(
		sections[1].keys()[0].values()[0]
			.attribute()
			.unwrap()
			.value(),
		"",
	);
	assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), OPAQUE);
}

#[test]
fn missing_attributes_are_distinct_from_empty_attributes() {
	let bank = parse(
		b"<Bank><Section><Key><Value/></Key><Key name=''/></Section><Section name=''/><Signature/></Bank>",
		Limits::default(),
	).unwrap();
	assert_eq!(bank.version(), None);
	assert_eq!(bank.sections()[0].name(), None);
	assert_eq!(bank.sections()[1].name(), Some(""));
	let keys = bank.sections()[0].keys();
	assert_eq!(keys[0].name(), None);
	assert_eq!(
		keys[0].values()[0].opaque_reason(),
		Some(OpaqueReason::MissingType)
	);
	assert_eq!(keys[1].name(), Some(""));
	assert_eq!(bank.signature().unwrap().value(), None);

	let signed = parse(
		b"<Bank version=''><Signature value='' /></Bank>",
		Limits::default(),
	)
	.unwrap();
	assert_eq!(signed.version(), Some(""));
	assert_eq!(signed.signature().unwrap().value(), Some(""));
}

#[test]
fn ambiguity_can_be_inspected_and_exported_unchanged() {
	let bank = parse(AMBIGUOUS, Limits::default()).unwrap();
	let value = &bank.sections()[0].keys()[0].values()[0];
	assert!(value.attribute().is_none());
	assert_eq!(value.opaque_reason(), Some(OpaqueReason::AmbiguousType));
	assert_eq!(
		&bank.source()[value.source_range()],
		br#"<Value int="0" fixed="0.0"/>"#,
	);
	assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), AMBIGUOUS);
	assert!(matches!(
		serialize(
			&bank,
			&[ValueEdit { id: value.id(), value: "2" }],
			Limits::default(),
		),
		Err(SerializeError::UnsupportedValue(id)) if id == value.id(),
	));
}

#[test]
fn namespaced_and_structured_child_values_remain_opaque() {
	let source = br#"<Bank xmlns:x='urn:namespace'><Section name='S'><Key name='K'>
		<x:Value string='a'/><Value x:string='b'/><Value xmlns:y='urn:other' string='c'/>
		<Value string='d'><Nested int='42'/></Value><Value string='e'>payload</Value>
	</Key></Section></Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	let values = bank.sections()[0].keys()[0].values();
	assert_eq!(values[0].namespace(), Some("urn:namespace"));
	assert!(values.iter().all(|value| value.attribute().is_none()));
	let reasons: Vec<_> = values
		.iter()
		.map(|node| node.opaque_reason().unwrap())
		.collect();
	assert_eq!(
		reasons,
		[
			OpaqueReason::Namespace,
			OpaqueReason::Namespace,
			OpaqueReason::Namespace,
			OpaqueReason::StructuredContent,
			OpaqueReason::StructuredContent,
		],
	);
	assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), source);
}

#[test]
fn qualified_structural_nodes_and_attributes_cannot_be_mistaken_for_unqualified_names() {
	let source = br#"<Bank xmlns:x='urn:extension' x:version='extension' version='1'>
		<x:Section name='unprojected'/>
		<Section x:name='extension' name='plain'>
			<x:Key name='unprojected'/>
			<Key x:name='extension'/>
			<Key x:name='extension' name='plain'/>
		</Section>
		<Signature x:value='extension'/>
		<x:Signature value='unprojected'/>
	</Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	assert_eq!(bank.version(), Some("1"));
	assert_eq!(bank.sections().len(), 1);
	assert_eq!(bank.sections()[0].name(), Some("plain"));
	let keys = bank.sections()[0].keys();
	assert_eq!(keys.len(), 2);
	assert_eq!(keys[0].name(), None);
	assert_eq!(keys[1].name(), Some("plain"));
	assert_eq!(bank.signature().unwrap().value(), None);
	assert!(bank.has_unprojected_bank_nodes());
	assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), source);
}

#[test]
fn same_value_edit_preserves_entity_spelling_and_all_source_bytes() {
	let bank = parse(OPAQUE, Limits::default()).unwrap();
	let node = &bank.sections()[0].keys()[1].values()[0];
	let edit = ValueEdit {
		id: node.id(),
		value: "élan & A",
	};
	assert_eq!(
		serialize(&bank, &[edit], Limits::default()).unwrap(),
		OPAQUE
	);
}

#[test]
fn edits_rewrite_only_targeted_lexemes_and_round_trip_decoded_data() {
	let bank = parse(OPAQUE, Limits::default()).unwrap();
	let value_nodes = &bank.sections()[0].keys()[1].values();
	let first = &value_nodes[0];
	let second = &bank.sections()[1].keys()[0].values()[0];

	let first_change = "l'été < A & B\t\r\n";
	let second_change = "quoted \"< & >\" 🙂";
	let edits = [
		ValueEdit {
			id: second.id(),
			value: second_change,
		},
		ValueEdit {
			id: first.id(),
			value: first_change,
		},
	];
	let output = serialize(&bank, &edits, Limits::default()).unwrap();

	let expected = std::str::from_utf8(OPAQUE)
		.unwrap()
		.replace(
			"élan &amp; &#x41;",
			"l&apos;été &lt; A &amp; B&#x9;&#xD;&#xA;",
		)
		.replace("string=''", "string='quoted \"&lt; &amp; &gt;\" 🙂'");
	assert_eq!(output, expected.as_bytes());

	let reopened = parse(&output, Limits::default()).unwrap();
	let first_reopened = &reopened.sections()[0].keys()[1].values()[0];
	let second_reopened = &reopened.sections()[1].keys()[0].values()[0];
	assert_eq!(first_reopened.id(), first.id());
	assert_eq!(second_reopened.id(), second.id());
	assert_eq!(first_reopened.attribute().unwrap().value(), first_change);
	assert_eq!(second_reopened.attribute().unwrap().value(), second_change);
	assert_eq!(bank.source(), OPAQUE);
}

#[test]
fn replacing_an_attribute_with_wide_equals_spacing_uses_the_complete_range() {
	let spaces = " ".repeat(300);
	let raw = format!("<Bank><Section><Key><Value string{spaces}='foo'/></Key></Section></Bank>");
	let bank = parse(raw.as_bytes(), Limits::default()).unwrap();
	let node = &bank.sections()[0].keys()[0].values()[0];
	assert_eq!(
		&bank.source()[node.attribute().unwrap().source_range()],
		b"foo"
	);
	let output = serialize(
		&bank,
		&[ValueEdit {
			id: node.id(),
			value: "replace",
		}],
		Limits::default(),
	)
	.unwrap();
	assert_eq!(output, raw.replace("foo", "replace").into_bytes());
}

#[test]
fn xml_attribute_decoding_preserves_numeric_whitespace_references() {
	let bank = parse(WHITESPACE, Limits::default()).unwrap();
	let attribute = bank.sections()[0].keys()[0].values()[0]
		.attribute()
		.unwrap();
	assert_eq!(attribute.value(), "a b  c\td\ne\rf");
	assert_eq!(
		serialize(&bank, &[], Limits::default()).unwrap(),
		WHITESPACE
	);
}

#[test]
fn invalid_edits_fail_without_mutating_source() {
	let bank = parse(ORDINARY, Limits::default()).unwrap();
	let id = bank.sections()[0].keys()[0].values()[0].id();
	let edit = ValueEdit { id, value: "0" };
	assert!(matches!(
		serialize(&bank, &[edit, edit], Limits::default()),
		Err(SerializeError::DuplicateEdit(duplicate)) if duplicate == id,
	));
	assert!(matches!(
		serialize(
			&bank,
			&[ValueEdit { id, value: "\u{0}" }],
			Limits::default(),
		),
		Err(SerializeError::InvalidCharacter { id: invalid, character: '\u{0}' }) if invalid == id,
	));
	let opaque = parse(OPAQUE, Limits::default()).unwrap();
	let future_kind = opaque.sections()[0].keys()[1].values()[1].id();
	assert!(matches!(
		serialize(
			&opaque,
			&[ValueEdit { id: future_kind, value: "guess" }],
			Limits::default(),
		),
		Err(SerializeError::UnsupportedValue(target)) if target == future_kind,
	));
	assert_eq!(bank.source(), ORDINARY);
	assert_eq!(opaque.source(), OPAQUE);
}

#[test]
fn parse_errors_have_typed_categories_and_positions() {
	let checks: &[(&[u8], ParseErrorKind)] = &[
		(
			include_bytes!("fixtures/dtd.sc2bank"),
			ParseErrorKind::Doctype,
		),
		(
			include_bytes!("fixtures/truncated.sc2bank"),
			ParseErrorKind::InvalidXml,
		),
		(
			include_bytes!("fixtures/trailing.sc2bank"),
			ParseErrorKind::InvalidXml,
		),
		(
			include_bytes!("fixtures/surrogate-reference.sc2bank"),
			ParseErrorKind::InvalidXml,
		),
		(
			include_bytes!("fixtures/duplicate-signature.sc2bank"),
			ParseErrorKind::DuplicateSignature,
		),
		(b"<Other/>", ParseErrorKind::WrongRoot),
		(b"<Bank xmlns='urn:test'/>", ParseErrorKind::WrongRoot),
		(b"<Bank><Bad a='<'/></Bank>", ParseErrorKind::InvalidXml),
		(b"<Bank/><Bank/>", ParseErrorKind::InvalidXml),
		(b"<Bank><1InvalidName/></Bank>", ParseErrorKind::InvalidXml),
		(
			b"<Bank xmlns:x='urn:one' xmlns:y='urn:one' x:a='1' y:a='2'/>",
			ParseErrorKind::InvalidXml,
		),
		(b"<?XML version='1.0'?><Bank/>", ParseErrorKind::InvalidXml),
		(
			b"<?xml version='1.0' standalone='other'?><Bank/>",
			ParseErrorKind::InvalidXml,
		),
		(
			b"<?xml\tversion='1.0'encoding='utf-8'?><Bank/>",
			ParseErrorKind::InvalidXml,
		),
		(
			b"<?xml version='1.1'?><Bank/>",
			ParseErrorKind::UnsupportedXmlVersion,
		),
		(
			b"<?xml version='1.0' encoding='UTF-16'?><Bank/>",
			ParseErrorKind::UnsupportedEncoding,
		),
	];
	for (source, expected) in checks {
		let error = parse(source, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), *expected, "{error} for {source:?}");
		assert!(error.location().is_some(), "no location for {error}");
	}
}

#[test]
fn validator_diagnostics_use_xml_line_endings_and_character_columns() {
	for newline in ["\n", "\r", "\r\n"] {
		let source = format!("<Bank>{newline}<!-- π --><1/>{newline}</Bank>");
		let error = parse(source.as_bytes(), Limits::default()).unwrap_err();
		let location = error.location().unwrap();
		assert_eq!(error.kind(), ParseErrorKind::InvalidXml);
		assert_eq!(location.byte, source.find('1'), "{newline:?}");
		assert_eq!((location.line, location.column), (2, 12), "{newline:?}");
		assert_eq!(
			error.to_string(),
			"invalid XML at 2:12 (invalid name token)"
		);
	}
}

#[test]
fn processing_instruction_data_requires_xml_whitespace_after_the_target() {
	let valid: &[&[u8]] = &[
		b"<Bank><?test?></Bank>",
		b"<Bank><?test /?></Bank>",
		b"<?test\r\ndata?><Bank/><?other?>",
		"<?xml-stylesheet href='style.xml'?><Bank><?π-1 data?></Bank>".as_bytes(),
		b"<?xml\tversion='1.0'?><Bank/>",
		"\u{feff}<?xml\r\nversion='1.0'?><Bank/>".as_bytes(),
	];
	for source in valid {
		let bank = parse(source, Limits::default()).unwrap();
		assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), *source);
	}

	let invalid: &[&[u8]] = &[
		b"<Bank><?test*?></Bank>",
		b"<Bank><?test/?></Bank>",
		b"<Bank><?xml*?></Bank>",
		b"<Bank><?XmL?></Bank>",
		b"<?test*?><Bank/>",
		b"<Bank/><?xMl*?>",
		b"<Bank><?xml\tversion='1.0'?></Bank>",
	];
	for source in invalid {
		let error = parse(source, Limits::default())
			.expect_err("malformed processing instruction must fail");
		assert_eq!(error.kind(), ParseErrorKind::InvalidXml, "{source:?}");
		assert!(error.location().is_some(), "{source:?}");
	}
}

#[test]
fn each_resource_limit_rejects_before_export() {
	let cases: &[(&[u8], Limits, LimitKind)] = &[
		(
			b"<Bank/>",
			Limits {
				document_bytes: 6,
				..Limits::default()
			},
			LimitKind::DocumentBytes,
		),
		(
			b"<Bank><Nested/></Bank>",
			Limits {
				element_depth: 1,
				..Limits::default()
			},
			LimitKind::ElementDepth,
		),
		(
			b"<Bank><Nested/></Bank>",
			Limits {
				nodes: 1,
				..Limits::default()
			},
			LimitKind::Nodes,
		),
		(
			b"<Bank version='1' xmlns:a='urn:a'/>",
			Limits {
				attributes_per_element: 1,
				..Limits::default()
			},
			LimitKind::AttributesPerElement,
		),
		(
			b"<Bank value='1234567'/>",
			Limits {
				scalar_bytes: 6,
				..Limits::default()
			},
			LimitKind::ScalarBytes,
		),
		(
			b"<Bank><!-- === --></Bank>",
			Limits {
				markup_markers: 2,
				..Limits::default()
			},
			LimitKind::MarkupMarkers,
		),
	];
	for (source, limits, expected) in cases {
		let error = parse(source, *limits).unwrap_err();
		assert_eq!(
			error.kind(),
			ParseErrorKind::LimitExceeded(*expected),
			"{error}"
		);
	}

	let bank = parse(ORDINARY, Limits::default()).unwrap();
	let id = bank.sections()[0].keys()[0].values()[0].id();
	let oversized = ValueEdit {
		id,
		value: "&&&&&&",
	};
	let limits = Limits {
		document_bytes: ORDINARY.len(),
		..Limits::default()
	};
	assert!(matches!(
		serialize(&bank, &[oversized], limits),
		Err(SerializeError::LimitExceeded(LimitKind::DocumentBytes)),
	));
}
