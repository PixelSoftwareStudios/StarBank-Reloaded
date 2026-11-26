use sc2bank::{LimitKind, Limits, ParseErrorKind, SerializeError, parse, replace_signature};

const DIGEST: &str = "9689A338F3C1C57C9090E476B6A5F60336C17DAC";

#[test]
fn replaces_only_the_unqualified_value_payload() {
	let limits = Limits::default();
	let source = concat!(
		"\u{feff}<?xml version='1.0'?>\r\n<Bank version = '01'>",
		"<!-- prefix --><Signature xmlns:n='urn:test' n:value='opaque' value = '&#x41;&amp;' note = 'left'>",
		"<!-- inside --></Signature><Section name='Repeated'/><Section name='Repeated'/>",
		"</Bank ><!-- suffix -->\r\n",
	);
	let expected = source.replace("&#x41;&amp;", DIGEST);
	let original = parse(source.as_bytes(), limits).unwrap();
	assert_eq!(original.signature().unwrap().value(), Some("A&"));
	let edited = replace_signature(&original, DIGEST, limits).unwrap();
	assert_eq!(edited.source(), expected.as_bytes());
	assert_eq!(edited.signature().unwrap().value(), Some(DIGEST));
	assert_eq!(original.source(), source.as_bytes());
	assert_eq!(original.sections()[0].id(), edited.sections()[0].id());
}

#[test]
fn adds_a_value_to_a_signature_without_overwriting_its_other_attributes() {
	let limits = Limits::default();
	let source = "<Bank><Signature xmlns:x='urn:test' x:value='different' note='kept' /></Bank>";
	let expected = source.replace("<Signature", &format!("<Signature value=\"{DIGEST}\""));
	let original = parse(source.as_bytes(), limits).unwrap();
	assert_eq!(original.signature().unwrap().value(), None);
	let edited = replace_signature(&original, DIGEST, limits).unwrap();
	assert_eq!(edited.source(), expected.as_bytes());
	assert_eq!(edited.signature().unwrap().value(), Some(DIGEST));
}

#[test]
fn inserts_before_the_actual_root_end_and_keeps_the_rest_byte_identical() {
	let limits = Limits::default();
	let source = concat!(
		"\u{feff}<!-- <Bank/> -->\n<Bank version='1'>\r\n",
		"<Section name='S'/>\r\n",
		"<!-- </Bank> -->\r\n</Bank \n>\r\n<!-- </Bank> -->",
	);
	let expected = source.replace(
		"</Bank \n>",
		&format!("<Signature value=\"{DIGEST}\"/></Bank \n>"),
	);
	let original = parse(source.as_bytes(), limits).unwrap();
	let edited = replace_signature(&original, DIGEST, limits).unwrap();
	assert_eq!(edited.source(), expected.as_bytes());
	assert_eq!(edited.signature().unwrap().value(), Some(DIGEST));
	assert_eq!(original.sections()[0].id(), edited.sections()[0].id());
	assert!(original.signature().is_none());
}

#[test]
fn expands_only_the_empty_root_closer_to_contain_a_signature() {
	let limits = Limits::default();
	let source = "<?xml version='1.0'?><Bank version = '27' /><!-- after -->";
	let expected = format!(
		"<?xml version='1.0'?><Bank version = '27' ><Signature value=\"{DIGEST}\"/></Bank><!-- after -->"
	);
	let original = parse(source.as_bytes(), limits).unwrap();
	let edited = replace_signature(&original, DIGEST, limits).unwrap();
	assert_eq!(edited.source(), expected.as_bytes());
	assert_eq!(edited.version(), Some("27"));
	assert_eq!(edited.signature().unwrap().value(), Some(DIGEST));
}

#[test]
fn handles_an_empty_value_and_preserves_exact_already_prepared_bytes() {
	let limits = Limits::default();
	let source = "<Bank><Signature value = '' /></Bank>";
	let original = parse(source.as_bytes(), limits).unwrap();
	let edited = replace_signature(&original, DIGEST, limits).unwrap();
	let expected = source.replace("''", &format!("'{DIGEST}'"));
	assert_eq!(edited.source(), expected.as_bytes());
	let again = replace_signature(&edited, DIGEST, limits).unwrap();
	assert_eq!(again.source(), expected.as_bytes());
}

#[test]
fn validates_digest_and_reparse_limits_before_returning_a_document() {
	let original = parse(b"<Bank/>", Limits::default()).unwrap();
	for bad in [
		"",
		"<unsafe>",
		&DIGEST.to_ascii_lowercase(),
		&"G".repeat(DIGEST.len()),
	] {
		assert!(matches!(
			replace_signature(&original, bad, Limits::default()),
			Err(SerializeError::InvalidSignature)
		));
	}

	for (limits, kind) in [
		(
			Limits {
				document_bytes: original.source().len(),
				..Limits::default()
			},
			LimitKind::DocumentBytes,
		),
		(
			Limits {
				scalar_bytes: DIGEST.len() - 1,
				..Limits::default()
			},
			LimitKind::ScalarBytes,
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
		match replace_signature(&original, DIGEST, limits).unwrap_err() {
			SerializeError::LimitExceeded(found) => assert_eq!(found, kind),
			SerializeError::InvalidOutput(error) => {
				assert_eq!(error.kind(), ParseErrorKind::LimitExceeded(kind));
			}
			error => panic!("unexpected signature edit error: {error}"),
		}
	}
	assert_eq!(original.source(), b"<Bank/>");
	assert!(original.signature().is_none());
}
