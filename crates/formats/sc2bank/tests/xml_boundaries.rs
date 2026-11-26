use std::fmt::Write as _;

use sc2bank::{LimitKind, Limits, ParseErrorKind, ValueEdit, parse, serialize};

#[test]
fn empty_default_namespace_is_unqualified_and_preserved() {
	let source = br#"<Bank xmlns='' xmlns:x='urn:foreign'><Section xmlns=''><Key><Value int='0001'/><Text string='A &amp; B'/></Key></Section><x:Section xmlns=''/></Bank>"#;
	let bank = parse(source, Limits::default()).unwrap();
	assert_eq!(bank.sections().len(), 1);
	assert!(bank.has_unprojected_bank_nodes());
	let values = bank.sections()[0].keys()[0].values();
	assert_eq!(values[0].namespace(), None);
	assert_eq!(values[0].attribute().unwrap().value(), "0001");
	assert_eq!(values[1].attribute().unwrap().value(), "A & B");
	assert_eq!(serialize(&bank, &[], Limits::default()).unwrap(), source);
	let changed = serialize(
		&bank,
		&[ValueEdit {
			id: values[0].id(),
			value: "0010",
		}],
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		changed,
		std::str::from_utf8(source)
			.unwrap()
			.replace("0001", "0010")
			.into_bytes(),
	);
}

#[test]
fn unsupported_octet_signatures_get_named_refusals() {
	let cases: &[(&[u8], &str)] = &[
		(&[0, b'<', 0, b'?'], "UTF-16BE"),
		(&[b'<', 0, b'?', 0], "UTF-16LE"),
		(&[0, 0, 0xfe, 0xff, 0, 0, 0, b'<'], "UTF-32BE"),
		(&[0xff, 0xfe, 0, 0, b'<', 0, 0, 0], "UTF-32LE"),
		(&[0, 0, 0, b'<', 0, 0, 0, b'B'], "UTF-32BE"),
		(&[b'<', 0, 0, 0, b'B', 0, 0, 0], "UTF-32LE"),
		(&[0, 0, 0xff, 0xfe], "UCS-4 (2143 byte order)"),
		(&[0xfe, 0xff, 0, 0], "UCS-4 (3412 byte order)"),
		(&[0x4c, 0x6f, 0xa7, 0x94], "EBCDIC-family XML"),
	];
	for &(bytes, encoding) in cases {
		let error = parse(bytes, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::UnsupportedEncoding);
		assert_eq!(error.location().unwrap().byte, Some(0));
		assert!(error.to_string().contains(encoding), "{error}");
	}
}

#[test]
fn non_utf8_declarations_are_identified_without_transcoding() {
	for source in [
		br#"<?xml version="1.0" encoding="windows-1252"?><Bank/>"#.to_vec(),
		b"<?xml version='1.0' encoding='windows-1252'?><Bank><!-- \xe9 --></Bank>".to_vec(),
	] {
		let error = parse(&source, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::UnsupportedEncoding);
		assert!(error.to_string().contains("declared encoding windows-1252"));
	}

	let long_name = "A".repeat(1000);
	let source = format!("<?xml version='1.0' encoding='{long_name}'?><Bank/>");
	let error = parse(source.as_bytes(), Limits::default()).unwrap_err();
	assert_eq!(error.kind(), ParseErrorKind::UnsupportedEncoding);
	assert!(error.to_string().len() < 160);
	assert!(error.to_string().contains('…'));
}

#[test]
fn malformed_encoding_labels_are_not_treated_as_supported_format_identifiers() {
	for label in ["", " UTF-8", "8-BIT", "UTF 8", "按"] {
		let source = format!("<?xml version='1.0' encoding='{label}'?><Bank/>");
		let error = parse(source.as_bytes(), Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::InvalidXml, "{source}");
	}
	let duplicate = br#"<?xml version="1.0" encoding="unknown" encoding="UTF-8"?><Bank/>"#;
	let error = parse(duplicate, Limits::default()).unwrap_err();
	assert_eq!(error.kind(), ParseErrorKind::InvalidXml);
}

#[test]
fn utf8_bom_does_not_shift_columns_and_is_preserved_in_exports() {
	let source = "\u{feff}<?xml\tversion='1.0' encoding='uTf-8'?>\r\n<Bank><Section><Key><Value string='雪 &#x1F680;'/></Key></Section></Bank>";
	let original = parse(source.as_bytes(), Limits::default()).unwrap();
	assert_eq!(
		serialize(&original, &[], Limits::default()).unwrap(),
		source.as_bytes()
	);
	let value = &original.sections()[0].keys()[0].values()[0];
	assert_eq!(value.attribute().unwrap().value(), "雪 🚀");
	let edited = serialize(
		&original,
		&[ValueEdit {
			id: value.id(),
			value: "火 & 🚀",
		}],
		Limits::default(),
	)
	.unwrap();
	assert_eq!(
		edited,
		source.replace("雪 &#x1F680;", "火 &amp; 🚀").into_bytes()
	);

	let error = parse("\u{feff}<NotBank/>".as_bytes(), Limits::default()).unwrap_err();
	let position = error.location().unwrap();
	assert_eq!(
		(position.byte, position.line, position.column),
		(Some(3), 1, 1)
	);

	let error = parse(b"\xef\xbb\xbf<Bank><!-- \xff --></Bank>", Limits::default()).unwrap_err();
	assert_eq!(error.kind(), ParseErrorKind::UnsupportedEncoding);
	let position = error.location().unwrap();
	assert_eq!(
		(position.byte, position.line, position.column),
		(Some(14), 1, 12)
	);
}

#[test]
fn namespace_projection_refuses_unrepresentable_and_illegal_declarations() {
	let misscoped = br#"<Bank xmlns:x='urn:foreign'><Section x:xmlns='urn:different'/></Bank>"#;
	let error = parse(misscoped, Limits::default()).unwrap_err();
	assert_eq!(error.kind(), ParseErrorKind::UnsupportedNamespace);
	assert!(error.to_string().contains("prefixed xmlns attribute"));
	let undeclaration = br#"<Bank xmlns:x=''><x:Section/></Bank>"#;
	let error = parse(undeclaration, Limits::default()).unwrap_err();
	assert_eq!(error.kind(), ParseErrorKind::InvalidXml);
	assert!(error.to_string().contains("cannot be undeclared"));
}

#[test]
fn namespace_declaration_budget_is_bounded_across_siblings() {
	const ALLOWED: usize = 1024;
	let mut body = String::from("<Bank>");
	for index in 0..ALLOWED {
		write!(body, "<Foreign xmlns:e='urn:example:{index}'/>").unwrap();
	}
	let accepted = format!("{body}</Bank>");
	let bank = parse(accepted.as_bytes(), Limits::default()).unwrap();
	assert_eq!(
		serialize(&bank, &[], Limits::default()).unwrap(),
		accepted.as_bytes()
	);

	body.push_str("<Foreign xmlns:e='urn:last'/></Bank>");
	let error = parse(body.as_bytes(), Limits::default()).unwrap_err();
	assert_eq!(
		error.kind(),
		ParseErrorKind::LimitExceeded(LimitKind::NamespaceDeclarations),
	);
}

#[test]
fn namespace_scope_budget_counts_inheritance_and_releases_siblings() {
	const ATTRIBUTES_PER_ELEMENT: usize = 64;
	let mut outer = String::from("<Bank");
	let mut middle = String::from("<Foreign");
	for index in 0..ATTRIBUTES_PER_ELEMENT {
		write!(outer, " xmlns:a{index}='urn:a:{index}'").unwrap();
		write!(middle, " xmlns:b{index}='urn:b:{index}'").unwrap();
	}
	let accepted = format!("{outer}>{middle}><Leaf/></Foreign>{middle}/></Bank>");
	let bank = parse(accepted.as_bytes(), Limits::default()).unwrap();
	assert_eq!(bank.source(), accepted.as_bytes());

	let rejected = format!("{outer}>{middle}><Leaf xmlns:c='urn:c'/></Foreign></Bank>");
	let error = parse(rejected.as_bytes(), Limits::default()).unwrap_err();
	assert_eq!(
		error.kind(),
		ParseErrorKind::LimitExceeded(LimitKind::NamespaceScope),
	);
}

#[test]
fn document_budget_and_dtd_refusals_still_apply_before_projection() {
	let unsupported = [0xff, 0xfe, 0, 0, b'<', 0, 0, 0];
	let error = parse(
		&unsupported,
		Limits {
			document_bytes: unsupported.len() - 1,
			..Limits::default()
		},
	)
	.unwrap_err();
	assert_eq!(
		error.kind(),
		ParseErrorKind::LimitExceeded(LimitKind::DocumentBytes)
	);

	let dtd = "\u{feff}<!DOCTYPE Bank SYSTEM 'file:///unused.dtd'><Bank/>";
	let error = parse(dtd.as_bytes(), Limits::default()).unwrap_err();
	assert_eq!(error.kind(), ParseErrorKind::Doctype);
	assert_eq!(error.location().unwrap().byte, Some(3));
}
