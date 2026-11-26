use sc2bank::{
	LimitKind, Limits, ParseErrorKind, SerializeError, StructuralEdit, StructuralNodeId, ValueEdit,
	ValueKind, XmlEncoding, apply_structural_edit, parse, replace_signature, serialize,
};

const UTF16: [XmlEncoding; 2] = [XmlEncoding::Utf16Le, XmlEncoding::Utf16Be];
const DIGEST: &str = "00112233445566778899AABBCCDDEEFF00112233";

fn units(text: &str, encoding: XmlEncoding) -> Vec<u8> {
	text.encode_utf16()
		.flat_map(|unit| match encoding {
			XmlEncoding::Utf16Le => unit.to_le_bytes(),
			XmlEncoding::Utf16Be => unit.to_be_bytes(),
			XmlEncoding::Utf8 => unreachable!("UTF-16 test helper"),
		})
		.collect()
}

fn native(text: &str, encoding: XmlEncoding) -> Vec<u8> {
	let marked = if text.starts_with('\u{feff}') {
		text.to_owned()
	} else {
		format!("\u{feff}{text}")
	};
	units(&marked, encoding)
}

#[test]
fn supplied_utf16le_fixture_retains_its_native_bytes_and_declared_encoding() {
	let bytes = include_bytes!("fixtures/ordinary-utf16le.sc2bank");
	let doc = parse(bytes, Limits::default()).unwrap();
	assert_eq!(doc.encoding(), XmlEncoding::Utf16Le);
	assert!(
		doc.xml_text()
			.starts_with("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-16\"?>")
	);
	assert_eq!(doc.source(), bytes);
	assert_eq!(serialize(&doc, &[], Limits::default()).unwrap(), bytes);
	assert_eq!(
		doc.encode_xml_text(doc.xml_text(), Limits::default())
			.unwrap(),
		bytes
	);
}

#[test]
fn utf16_native_spans_cover_exact_elements_and_attribute_spelling() {
	for encoding in UTF16 {
		let value_element = "<Value string = '雪🙂 &#x41; &amp;' />";
		let key_element = format!("<Key name='λ'>{value_element}</Key>");
		let section_element = format!("<Section name='雪'>{key_element}</Section>");
		let source = format!(
			"<?xml version='1.0' encoding='UTF-16'?>\r\n<!--{}-->\r\n<Bank>{section_element}<Signature value='unverified'/></Bank>",
			"雪🚀a".repeat(80),
		);
		let bytes = native(&source, encoding);
		let doc = parse(&bytes, Limits::default()).unwrap();
		assert_eq!(doc.encoding(), encoding);
		assert_eq!(doc.xml_text(), format!("\u{feff}{source}"));
		let section = &doc.sections()[0];
		let key = &section.keys()[0];
		let value = &key.values()[0];
		let attribute = value.attribute().unwrap();
		assert_eq!(attribute.value(), "雪🙂 A &");
		assert_eq!(
			&bytes[section.source_range()],
			units(&section_element, encoding)
		);
		assert_eq!(&bytes[key.source_range()], units(&key_element, encoding));
		assert_eq!(&bytes[value.source_range()], units(value_element, encoding));
		assert_eq!(
			&bytes[attribute.source_range()],
			units("雪🙂 &#x41; &amp;", encoding)
		);
		assert_eq!(
			&bytes[doc.signature().unwrap().source_range()],
			units("<Signature value='unverified'/>", encoding),
		);
	}
}

#[test]
fn value_edit_keeps_utf16_endian_and_unrelated_bytes() {
	for encoding in UTF16 {
		let source = "<?xml version='1.0' encoding='UTF-16'?>\r\n<Bank><!--雪--><Section><Key><Value string='é &#x41; 🙂'/><Future future='raw'/></Key></Section></Bank>";
		let input = native(source, encoding);
		let doc = parse(&input, Limits::default()).unwrap();
		let value = &doc.sections()[0].keys()[0].values()[0];
		let unchanged = serialize(
			&doc,
			&[ValueEdit {
				id: value.id(),
				value: "é A 🙂",
			}],
			Limits::default(),
		)
		.unwrap();
		assert_eq!(unchanged, input);
		let output = serialize(
			&doc,
			&[ValueEdit {
				id: value.id(),
				value: "火 & ' \r\n🚀",
			}],
			Limits::default(),
		)
		.unwrap();
		assert_eq!(
			output,
			native(
				&source.replace("é &#x41; 🙂", "火 &amp; &apos; &#xD;&#xA;🚀"),
				encoding
			),
		);
		let reopened = parse(&output, Limits::default()).unwrap();
		let value = &reopened.sections()[0].keys()[0].values()[0];
		assert_eq!(reopened.encoding(), encoding);
		assert_eq!(value.attribute().unwrap().value(), "火 & ' \r\n🚀");
	}
}

#[test]
fn structural_edits_shift_identities_in_native_utf16_coordinates() {
	for encoding in UTF16 {
		let source = "<Bank><Section name='S'><Key name='K'><First string='雪'/><Last fixed='1.00'/></Key></Section><Signature value='unverified'/></Bank>";
		let doc = parse(&native(source, encoding), Limits::default()).unwrap();
		let section = &doc.sections()[0];
		let key = &section.keys()[0];
		let removed = key.values()[0].id();
		let saved = key.values()[1].id();
		let renamed = apply_structural_edit(
			&doc,
			StructuralEdit::RenameSection {
				id: section.id(),
				name: "火 🚀",
			},
			Limits::default(),
		)
		.unwrap();
		let old_value_id = renamed.remap.value(removed).unwrap();
		let saved_value_id = renamed.remap.value(saved).unwrap();
		let deleted = apply_structural_edit(
			&renamed.document,
			StructuralEdit::RemoveValue { id: old_value_id },
			Limits::default(),
		)
		.unwrap();
		assert!(deleted.remap.value(old_value_id).is_none());
		let key = &deleted.document.sections()[0].keys()[0];
		assert_eq!(
			deleted.remap.value(saved_value_id),
			Some(key.values()[0].id())
		);
		let added = apply_structural_edit(
			&deleted.document,
			StructuralEdit::AddValue {
				key: key.id(),
				name: "火",
				kind: &ValueKind::String,
				value: "🙂",
			},
			Limits::default(),
		)
		.unwrap();
		let key = &added.document.sections()[0].keys()[0];
		assert_eq!(
			added.added,
			Some(StructuralNodeId::Value(key.values()[1].id()))
		);
		assert_eq!(
			added.document.source(),
			native(
				"<Bank><Section name='火 🚀'><Key name='K'><Last fixed='1.00'/><火 string=\"🙂\"/></Key></Section><Signature value='unverified'/></Bank>",
				encoding,
			),
		);
	}
}

#[test]
fn signature_insert_and_replace_preserve_utf16_order_and_bom() {
	for encoding in UTF16 {
		for (source, expected) in [
			(
				"<Bank version='日本' />",
				format!("<Bank version='日本' ><Signature value=\"{DIGEST}\"/></Bank>"),
			),
			(
				"<Bank><!--🙂--><Signature x='雪'/></Bank>",
				format!("<Bank><!--🙂--><Signature value=\"{DIGEST}\" x='雪'/></Bank>"),
			),
			(
				"<Bank><!--🙂--><Signature value = 'old'/></Bank>",
				format!("<Bank><!--🙂--><Signature value = '{DIGEST}'/></Bank>"),
			),
		] {
			let input = native(source, encoding);
			let doc = parse(&input, Limits::default()).unwrap();
			let signed = replace_signature(&doc, DIGEST, Limits::default()).unwrap();
			assert_eq!(signed.source(), native(&expected, encoding));
			assert_eq!(signed.encoding(), encoding);
			assert_eq!(signed.signature().unwrap().value(), Some(DIGEST));
			assert_eq!(doc.source(), input);
		}
	}
}

#[test]
fn raw_utf8_draft_reuses_native_byte_order_and_required_bom() {
	for encoding in UTF16 {
		let doc = parse(
			&native("<?xml version='1.0' encoding='UTF-16'?><Bank/>", encoding),
			Limits::default(),
		)
		.unwrap();
		let edited = doc
			.xml_text()
			.replace("<Bank/>", "<Bank><Section name='雪'/></Bank>");
		let detached_bom = edited.strip_prefix('\u{feff}').unwrap();
		for draft in [&edited, detached_bom] {
			let output = doc.encode_xml_text(draft, Limits::default()).unwrap();
			assert_eq!(output, native(detached_bom, encoding));
			assert_eq!(
				parse(&output, Limits::default()).unwrap().xml_text(),
				edited
			);
		}
	}
}

#[test]
fn malformed_or_conflicting_utf16_is_refused_with_native_offsets() {
	for encoding in UTF16 {
		let mut broken = native("<Bank>\r\n", encoding);
		let unpaired_position = broken.len();
		broken.extend(match encoding {
			XmlEncoding::Utf16Le => 0xd800u16.to_le_bytes(),
			_ => 0xd800u16.to_be_bytes(),
		});
		broken.extend(units("</Bank>", encoding));
		let error = parse(&broken, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::InvalidXml);
		assert_eq!(error.location().unwrap().byte, Some(unpaired_position));
		assert!(error.to_string().contains("surrogate"));

		let mut partial = native("<Bank/>", encoding);
		partial.push(0);
		let error = parse(&partial, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::InvalidXml);
		assert_eq!(error.location().unwrap().byte, Some(partial.len() - 1));

		let other = if encoding == XmlEncoding::Utf16Le {
			"UTF-16BE"
		} else {
			"UTF-16LE"
		};
		for declared in ["utf-8", other] {
			let conflicting = native(
				&format!("<?xml version='1.0' encoding='{declared}'?><Bank/>"),
				encoding,
			);
			let error = parse(&conflicting, Limits::default()).unwrap_err();
			assert_eq!(error.kind(), ParseErrorKind::UnsupportedEncoding);
			assert_eq!(error.location().unwrap().byte, Some(2));
		}

		let explicit = native(
			&format!(
				"<?xml version='1.0' encoding='{}'?><Bank/>",
				encoding.as_str()
			),
			encoding,
		);
		assert_eq!(
			parse(&explicit, Limits::default()).unwrap().encoding(),
			encoding
		);
	}
}

#[test]
fn decoded_xml_errors_and_dtd_rejection_use_native_byte_offsets() {
	for encoding in UTF16 {
		let prefix = "<Bank>\r\n<!--雪🙂--><";
		let invalid = native(&format!("{prefix}1/></Bank>"), encoding);
		let error = parse(&invalid, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::InvalidXml);
		let position = error.location().unwrap();
		assert_eq!(position.byte, Some(native(prefix, encoding).len()));
		assert_eq!(position.line, 2);

		let dtd = native(
			"<!DOCTYPE Bank SYSTEM 'urn:never-resolved'><Bank/>",
			encoding,
		);
		let error = parse(&dtd, Limits::default()).unwrap_err();
		assert_eq!(error.kind(), ParseErrorKind::Doctype);
		assert_eq!(error.location().unwrap().byte, Some(2));
	}
}

#[test]
fn document_limits_apply_to_native_and_working_utf8_sizes() {
	let larger_working_text = format!("<Bank><!--{}--></Bank>", "雪".repeat(100));
	for encoding in UTF16 {
		let bytes = native(&larger_working_text, encoding);
		let limit = Limits {
			document_bytes: bytes.len(),
			..Limits::default()
		};
		let error = parse(&bytes, limit).unwrap_err();
		assert_eq!(
			error.kind(),
			ParseErrorKind::LimitExceeded(LimitKind::DocumentBytes)
		);

		let source = "<Bank><Section><Key><Value string='a'/></Key></Section></Bank>";
		let bytes = native(source, encoding);
		let limits = Limits {
			document_bytes: bytes.len(),
			..Limits::default()
		};
		let doc = parse(&bytes, limits).unwrap();
		let value = &doc.sections()[0].keys()[0].values()[0];
		assert!(matches!(
			serialize(
				&doc,
				&[ValueEdit {
					id: value.id(),
					value: "abc"
				}],
				limits
			),
			Err(SerializeError::LimitExceeded(LimitKind::DocumentBytes)),
		));
	}
}
