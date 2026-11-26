use sc2bank::{LimitKind, Limits, SerializeError, parse};
use starbank_signature::{
	ComputeSignatureError, ResignError, SignatureStatus, SigningContext, resign, verify_signature,
};

const XML: &str = include_str!("fixtures/canonical-ascii.sc2bank");
const DIGEST: &str = "9689A338F3C1C57C9090E476B6A5F60336C17DAC";
const RENAMED_DIGEST: &str = "7E90277D5F908E05AF2E404147213787534CE90F";

fn context(name: &str) -> SigningContext {
	SigningContext::new("1-S2-1-0", "1-S2-1-0", name).unwrap()
}

#[test]
fn signs_and_checks_the_independent_digest_after_reparse() {
	let limits = Limits::default();
	let original = parse(XML.as_bytes(), limits).unwrap();
	let context = context("Fixture");
	let signed = resign(&original, &context, limits).unwrap();
	let expected = XML.replace(
		"</Bank>",
		&format!("<Signature value=\"{DIGEST}\"/></Bank>"),
	);
	assert_eq!(signed.source(), expected.as_bytes());
	let reopened = parse(signed.source(), limits).unwrap();
	assert_eq!(
		verify_signature(&reopened, Some(&context)),
		SignatureStatus::Valid,
	);
	assert_eq!(original.source(), XML.as_bytes());
	assert!(original.signature().is_none());
}

#[test]
fn changes_the_existing_digest_when_the_destination_context_changes() {
	let limits = Limits::default();
	let old_context = context("Fixture");
	let renamed_context = context("Renamed");
	let signed_source = XML.replace(
		"</Bank>",
		&format!("<Signature value = '{DIGEST}' /></Bank>"),
	);
	let original = parse(signed_source.as_bytes(), limits).unwrap();
	assert_eq!(
		verify_signature(&original, Some(&old_context)),
		SignatureStatus::Valid,
	);
	let prepared = resign(&original, &renamed_context, limits).unwrap();
	let expected = signed_source.replace(DIGEST, RENAMED_DIGEST);
	assert_eq!(prepared.source(), expected.as_bytes());
	assert_eq!(
		verify_signature(&prepared, Some(&old_context)),
		SignatureStatus::Invalid,
	);
	assert_eq!(
		verify_signature(&prepared, Some(&renamed_context)),
		SignatureStatus::Valid,
	);
	assert_eq!(original.source(), signed_source.as_bytes());
}

#[test]
fn repairs_entity_encoded_malformed_digest_without_rewriting_the_element() {
	let limits = Limits::default();
	let malformed = XML.replace(
		"</Bank>",
		"<Signature value = '&apos;' note='kept' /></Bank>",
	);
	let original = parse(malformed.as_bytes(), limits).unwrap();
	let prepared = resign(&original, &context("Fixture"), limits).unwrap();
	assert_eq!(
		prepared.source(),
		malformed.replace("&apos;", DIGEST).as_bytes(),
	);
}

#[test]
fn rejects_unsupported_xml_and_output_limit_without_changing_the_input() {
	let limits = Limits::default();
	let signing_context = context("Fixture");
	let unsupported_source = b"<Bank><Opaque><Section name='Hidden'/></Opaque></Bank>";
	let unsupported = parse(unsupported_source, limits).unwrap();
	assert!(matches!(
		resign(&unsupported, &signing_context, limits),
		Err(ResignError::Compute(
			ComputeSignatureError::UnprojectedNodes
		)),
	));
	assert_eq!(unsupported.source(), unsupported_source);

	let original = parse(XML.as_bytes(), limits).unwrap();
	let tight_limits = Limits {
		document_bytes: original.source().len(),
		..limits
	};
	assert!(matches!(
		resign(&original, &signing_context, tight_limits),
		Err(ResignError::Serialize(SerializeError::LimitExceeded(
			LimitKind::DocumentBytes
		))),
	));
	assert_eq!(original.source(), XML.as_bytes());
}
