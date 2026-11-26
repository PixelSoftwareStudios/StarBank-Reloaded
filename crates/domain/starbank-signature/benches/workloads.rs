#![forbid(unsafe_code)]

use sc2bank::{LimitKind, Limits, SerializeError, parse};
use starbank_signature::{
	ResignError, SignatureStatus, SigningContext, compute_signature, resign, verify_signature,
};
use starbank_testkit::{BANK_WORKLOADS, measure, print_header, synthetic_bank};

fn main() {
	print_header();
	let context = SigningContext::new("1-S2-1-999", "1-S2-1-321", "SyntheticBank")
		.expect("valid synthetic signing context");
	for (size, keys) in BANK_WORKLOADS {
		benchmark_signing(size, &synthetic_bank(keys), &context);
	}
}

fn benchmark_signing(size: &str, bank: &[u8], context: &SigningContext) {
	let limits = Limits::default();
	let source = parse(bank, limits).expect("valid synthetic bank");
	let expected = compute_signature(&source, context).expect("canonical synthetic bank");
	measure(
		"compute_signature",
		size,
		bank.len(),
		|| (),
		|_| compute_signature(&source, context).expect("canonical synthetic bank"),
		|_, digest| assert_eq!(digest, expected),
	);
	measure(
		"resign_and_reparse",
		size,
		bank.len(),
		|| (),
		|_| resign(&source, context, limits).expect("signed output"),
		|_, signed| {
			assert_eq!(
				verify_signature(&signed, Some(context)),
				SignatureStatus::Valid
			);
			assert_eq!(source.source(), bank);
		},
	);
	let exact_limit = Limits {
		document_bytes: bank.len(),
		..limits
	};
	measure(
		"resign_output_byte_limit",
		size,
		bank.len(),
		|| (),
		|_| resign(&source, context, exact_limit),
		|_, result| {
			assert!(matches!(
				result,
				Err(ResignError::Serialize(SerializeError::LimitExceeded(
					LimitKind::DocumentBytes
				)))
			));
			assert_eq!(source.source(), bank);
		},
	);
}
