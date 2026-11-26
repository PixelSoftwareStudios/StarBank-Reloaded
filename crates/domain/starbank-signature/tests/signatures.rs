use sc2bank::{BankDocument, Limits, OpaqueReason, parse};
use starbank_signature::{
	ComputeSignatureError, ContextField, InvalidContext, MalformedSignature, ParseSignatureError,
	Signature, SignatureStatus, SigningContext, compute_signature, verify_signature,
};

const CASE_TIES: &str = include_str!("fixtures/case-ties.sc2bank");
const SWAPPED_TIES: &str = include_str!("fixtures/case-swapped-ties.sc2bank");
const ASCII: &str = include_str!("fixtures/canonical-ascii.sc2bank");
const ORDINAL: &str = include_str!("fixtures/canonical-ordinal.sc2bank");
const CASE_TIES_DIGEST: &str = "400CCD1D17B653D68BE2FE645A694FB016C3105D";
const TEST_AUTHOR: &str = "1-S2-1-1001";
const TEST_PLAYER: &str = "2-S2-1-2002";

fn bank(xml: &str) -> BankDocument {
	parse(xml.as_bytes(), Limits::default()).expect("synthetic XML should parse")
}

fn context(bank_name: &str) -> SigningContext {
	SigningContext::new(TEST_AUTHOR, TEST_PLAYER, bank_name).expect("valid synthetic context")
}

fn digest(document: &BankDocument, context: &SigningContext) -> String {
	compute_signature(document, context)
		.expect("signable synthetic bank")
		.to_string()
}

fn with_signature(xml: &str, signature: &str) -> String {
	xml.replace("</Bank>", &format!("<Signature{signature}/></Bank>"))
}

#[test]
fn canonical_order_retains_case_and_stable_duplicate_occurrences() {
	let document = bank(CASE_TIES);
	let signing_context = context("Bank");
	assert_eq!(digest(&document, &signing_context), CASE_TIES_DIGEST);
	assert_eq!(
		digest(&bank(SWAPPED_TIES), &signing_context),
		"5B7E8B16CEF62A229D8A659498571B039B4F7040"
	);
	assert_eq!(document.source(), CASE_TIES.as_bytes());
	assert_eq!(document.sections()[0].name(), Some("alpha"));
	assert_eq!(document.sections()[1].keys()[0].name(), Some("a"));
	assert_eq!(
		document.sections()[1].keys()[0].values()[0]
			.attribute()
			.expect("string attribute")
			.value(),
		"first"
	);
	assert!(document.signature().is_none());
}

#[test]
fn ascii_groups_sort_named_children_without_collapsing_keys() {
	let signing_context = SigningContext::new("1-S2-1-0", "1-S2-1-0", "Fixture").unwrap();
	let document = bank(ASCII);
	assert_eq!(
		digest(&document, &signing_context),
		"9689A338F3C1C57C9090E476B6A5F60336C17DAC"
	);
	let renamed = SigningContext::new("1-S2-1-0", "1-S2-1-0", "Renamed").unwrap();
	assert_eq!(
		digest(&document, &renamed),
		"7E90277D5F908E05AF2E404147213787534CE90F"
	);
}

#[test]
fn ordering_uses_utf16_and_ascii_fallback_uses_each_code_unit() {
	let signing_context = SigningContext::new("1-S2-1-0", "1-S2-1-0", "Fixture").unwrap();
	assert_eq!(
		digest(&bank(ORDINAL), &signing_context),
		"55C56FBE432B737EB534DE691B9A410375C4234F"
	);

	// Independently checked against the pinned upstream signer on .NET.
	let name_order = bank(concat!(
		"<Bank><Section name=\"\u{e000}\"><Key name=\"B\"/></Section>",
		"<Section name=\"\u{10000}\"><Key name=\"A\"/></Section></Bank>",
	));
	assert_eq!(
		digest(&name_order, &context("Café\u{10000}")),
		"C7EC865065A0E59D7D1E88120E9C5A4B9FFC2B10"
	);
}

#[test]
fn xml_decoding_preserves_referenced_whitespace_and_empty_value_shape() {
	let document = bank(concat!(
		"<Bank version=\"1\"><Section name=\"S\"><Key name=\"Zero\"><Value/></Key>",
		"<Key name=\"Raw\"><Value string=\"  A\r\n\tB&#x9;&#xA;&#xD; &amp; &#xE9;  \"/></Key>",
		"<Key name=\"Empty\"><Value string=\"\"/></Key><Key name=\"NoChild\"/></Section></Bank>",
	));
	assert_eq!(
		digest(&document, &context("Bank")),
		"924230B94A9E6BC0EF7CD99F2D3379437ECC2219"
	);
}

#[test]
fn version_stored_signature_and_text_payload_do_not_enter_digest() {
	let changed = CASE_TIES
		.replace("version=\"27\"", "version=\"1\"")
		.replace("text=\"omitted\"", "text=\"a completely different value\"");
	let changed = with_signature(&changed, " value=\"malformed\"");
	let signing_context = context("Bank");
	assert_eq!(digest(&bank(&changed), &signing_context), CASE_TIES_DIGEST);
	assert_ne!(
		digest(
			&bank(&changed.replace("Text=\"kept\"", "Text=\"changed\"")),
			&signing_context
		),
		CASE_TIES_DIGEST,
	);
	assert_ne!(digest(&bank(CASE_TIES), &context("bank")), CASE_TIES_DIGEST,);
	let reversed = SigningContext::new(TEST_PLAYER, TEST_AUTHOR, "Bank").unwrap();
	assert_ne!(digest(&bank(CASE_TIES), &reversed), CASE_TIES_DIGEST);
}

#[test]
fn opaque_or_unprojected_data_cannot_produce_a_digest() {
	let signing_context = context("Bank");
	let unknown = bank("<Bank><Extra><Section name=\"S\"/></Extra></Bank>");
	assert_eq!(
		compute_signature(&unknown, &signing_context),
		Err(ComputeSignatureError::UnprojectedNodes)
	);
	let unnamed = bank("<Bank><Section/></Bank>");
	assert_eq!(
		compute_signature(&unnamed, &signing_context),
		Err(ComputeSignatureError::MissingSectionName(
			unnamed.sections()[0].id()
		))
	);
	let unnamed = bank("<Bank><Section name=\"S\"><Key/></Section></Bank>");
	assert_eq!(
		compute_signature(&unnamed, &signing_context),
		Err(ComputeSignatureError::MissingKeyName(
			unnamed.sections()[0].keys()[0].id()
		))
	);
	for (value, reason) in [
		(
			"<Value int=\"1\" string=\"2\"/>",
			OpaqueReason::AmbiguousType,
		),
		(
			"<x:Value xmlns:x=\"urn:test\" int=\"1\"/>",
			OpaqueReason::Namespace,
		),
		(
			"<Value int=\"1\"><Extra/></Value>",
			OpaqueReason::StructuredContent,
		),
	] {
		let xml =
			format!("<Bank><Section name=\"S\"><Key name=\"K\">{value}</Key></Section></Bank>");
		let document = bank(&xml);
		assert_eq!(
			compute_signature(&document, &signing_context),
			Err(ComputeSignatureError::OpaqueValue {
				id: document.sections()[0].keys()[0].values()[0].id(),
				reason,
			})
		);
	}
}

#[test]
fn verification_distinguishes_all_stored_signature_states() {
	let signing_context = context("Bank");
	let verify = |xml: &str| verify_signature(&bank(xml), Some(&signing_context));
	assert_eq!(
		verify_signature(&bank(CASE_TIES), None),
		SignatureStatus::Unsigned
	);
	let lower = CASE_TIES_DIGEST.to_ascii_lowercase();
	let signed = with_signature(CASE_TIES, &format!(" value=\"{lower}\""));
	assert_eq!(verify(&signed), SignatureStatus::Valid);
	assert_eq!(
		verify_signature(&bank(&signed), None),
		SignatureStatus::MissingContext
	);
	assert_eq!(
		verify_signature(&bank(&signed), Some(&context("Renamed"))),
		SignatureStatus::Invalid
	);
	assert_eq!(
		verify(&with_signature(CASE_TIES, "")),
		SignatureStatus::Malformed(MalformedSignature::MissingValue)
	);
	assert_eq!(
		verify(&with_signature(CASE_TIES, " value=\"\"")),
		SignatureStatus::Malformed(MalformedSignature::InvalidDigest(
			ParseSignatureError::Length
		))
	);
	let non_hex = "g".repeat(40);
	assert_eq!(
		verify(&with_signature(CASE_TIES, &format!(" value=\"{non_hex}\""))),
		SignatureStatus::Malformed(MalformedSignature::InvalidDigest(
			ParseSignatureError::NonHex
		))
	);
	assert_eq!(
		verify(&with_signature(
			"<Bank><Extra/></Bank>",
			&format!(" value=\"{lower}\"")
		)),
		SignatureStatus::Unsupported(ComputeSignatureError::UnprojectedNodes)
	);
	let document = bank(&signed);
	assert_eq!(document.source(), signed.as_bytes());
	assert_eq!(document.signature().unwrap().value(), Some(lower.as_str()));
	assert_eq!(
		lower.parse::<Signature>().unwrap().to_string(),
		CASE_TIES_DIGEST
	);
}

#[test]
fn context_validation_keeps_exact_lexical_inputs() {
	let valid = SigningContext::new("02-S2-01-0002", "3-S2-4-006", "  Exact.Bank  ").unwrap();
	assert_eq!(valid.author(), "02-S2-01-0002");
	assert_eq!(valid.player(), "3-S2-4-006");
	assert_eq!(valid.bank_name(), "  Exact.Bank  ");
	assert_eq!(
		SigningContext::new("", TEST_PLAYER, "Bank"),
		Err(InvalidContext::Empty(ContextField::Author))
	);
	for invalid in [
		"2002",
		"2-s2-1-2002",
		"2-S2--2002",
		"2-S2-1-2002-3",
		"2-S2-1-٢",
	] {
		assert_eq!(
			SigningContext::new(TEST_AUTHOR, invalid, "Bank"),
			Err(InvalidContext::Handle(ContextField::Player))
		);
	}
	assert_eq!(
		SigningContext::new(TEST_AUTHOR, TEST_PLAYER, ""),
		Err(InvalidContext::Empty(ContextField::BankName))
	);
	for invalid in [".", "..", "dir/Bank", "dir\\Bank", "Bad\0Name"] {
		assert_eq!(
			SigningContext::new(TEST_AUTHOR, TEST_PLAYER, invalid),
			Err(InvalidContext::BankName)
		);
	}
}
