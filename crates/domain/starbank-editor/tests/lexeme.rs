use std::borrow::Cow;

use sc2bank::ValueKind;
use starbank_editor::{LexemeError, normalize_value};

#[test]
fn integer_edits_strip_only_valid_grouping() {
	let cases = [
		("1,000,000", "1000000"),
		("999,000", "999000"),
		("-00,010", "-00010"),
		("9007199254740993", "9007199254740993"),
		("-0", "-0"),
		(
			"1,000,000,000,000,000,000,000,000,000,000,000,000,000",
			"1000000000000000000000000000000000000000",
		),
	];

	for (input, expected) in cases {
		assert_eq!(
			normalize_value(&ValueKind::Int, input).as_deref(),
			Ok(expected),
			"integer input {input:?}"
		);
		assert_eq!(
			normalize_value(&ValueKind::Int, expected),
			Ok(Cow::Borrowed(expected)),
			"normalized integer {expected:?}"
		);
	}
}

#[test]
fn fixed_edits_preserve_sign_and_decimal_precision() {
	let cases = [
		("-1,234.50", "-1234.50"),
		("000,120.000", "000120.000"),
		(
			"9007199254740993.10000000000000000000000000001",
			"9007199254740993.10000000000000000000000000001",
		),
		("-0.00", "-0.00"),
		("1,234", "1234"),
	];

	for (input, expected) in cases {
		assert_eq!(
			normalize_value(&ValueKind::Fixed, input).as_deref(),
			Ok(expected),
			"fixed input {input:?}"
		);
		assert_eq!(
			normalize_value(&ValueKind::Fixed, expected).as_deref(),
			Ok(expected),
			"normalized fixed value {expected:?}"
		);
	}
}

#[test]
fn rejects_incomplete_or_malformed_numeric_edits() {
	let invalid_numbers = [
		"",
		"-",
		"+1",
		"--2",
		",123",
		"1,",
		"1,,2",
		"12,34",
		"1234,567",
		"1,234,56",
		"1 000",
		" 1000",
		"1000 ",
		"1\t000",
		"１,０００",
		"1e3",
		"NaN",
		"Infinity",
		"−5",
		"1_.5",
		"1,2.50",
		"1.2,000",
		".",
		".5",
		"1.",
		"1.2.3",
	];

	for input in invalid_numbers {
		assert_eq!(
			normalize_value(&ValueKind::Int, input),
			Err(LexemeError::InvalidInteger),
			"integer input {input:?}"
		);
		assert_eq!(
			normalize_value(&ValueKind::Fixed, input),
			Err(LexemeError::InvalidFixed),
			"fixed input {input:?}"
		);
	}

	assert_eq!(
		normalize_value(&ValueKind::Int, "1.0"),
		Err(LexemeError::InvalidInteger)
	);
}

#[test]
fn borrows_verbatim_text_and_plain_numeric_input() {
	let text = "  A, B & <C> \t雪\n";
	for kind in [ValueKind::String, ValueKind::Text] {
		assert!(matches!(normalize_value(&kind, text), Ok(Cow::Borrowed(value)) if value == text));
		assert!(matches!(normalize_value(&kind, ""), Ok(Cow::Borrowed(""))));
	}

	assert!(matches!(
		normalize_value(&ValueKind::Int, "9007199254740993"),
		Ok(Cow::Borrowed("9007199254740993"))
	));
	assert!(matches!(
		normalize_value(&ValueKind::Fixed, "-0.00"),
		Ok(Cow::Borrowed("-0.00"))
	));
}

#[test]
fn rejects_structured_edits_without_a_known_syntax() {
	let cases = [
		(ValueKind::Flag, "1"),
		(ValueKind::Point, "1,000"),
		(ValueKind::Unit, "Marine"),
		(ValueKind::Unknown("future".to_owned()), "1"),
	];

	for (kind, input) in cases {
		assert_eq!(
			normalize_value(&kind, input),
			Err(LexemeError::UnsupportedKind),
			"structured input of {kind:?}"
		);
	}
}
