use starbank_core::{ParseRevisionError, Revision};

#[test]
fn revisions_round_trip_beyond_javascript_safe_integers() {
	for value in ["0", "9007199254740993", "18446744073709551615"] {
		assert_eq!(value.parse::<Revision>().unwrap().to_string(), value);
	}
}

#[test]
fn revision_parser_rejects_noncanonical_and_out_of_range_inputs() {
	for value in ["", "00", "01", "+1", "-1", " 1", "1 ", "1.0", "１"] {
		assert_eq!(
			value.parse(),
			Err::<Revision, _>(ParseRevisionError::NonCanonical)
		);
	}
	assert_eq!(
		"18446744073709551616".parse(),
		Err::<Revision, _>(ParseRevisionError::Overflow)
	);
}

#[test]
fn revisions_advance_without_wrapping() {
	assert_eq!(Revision::INITIAL.checked_next(), Some("1".parse().unwrap()));
	let maximum = "18446744073709551615".parse::<Revision>().unwrap();
	let penultimate = "18446744073709551614".parse::<Revision>().unwrap();
	assert_eq!(penultimate.checked_next(), Some(maximum));
	assert_eq!(maximum.checked_next(), None);
}
