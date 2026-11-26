#![forbid(unsafe_code)]

use sc2bank::{LimitKind, Limits, ParseErrorKind};
use starbank_core::Revision;
use starbank_editor::{EditorError, ExportSigning, OccurrenceKind, Session, ValueChange};
use starbank_signature::{SignatureStatus, SigningContext};
use starbank_testkit::{BANK_WORKLOADS, measure, print_header, synthetic_bank};

const BULK_EDITS: usize = 128;
const GROUPED_VALUE: &str = "9,007,199,254,740,993";
const EXACT_VALUE: &str = "9007199254740993";

fn main() {
	print_header();
	let context = SigningContext::new("1-S2-1-999", "1-S2-1-321", "SyntheticBank")
		.expect("valid synthetic signing context");
	for (size, keys) in BANK_WORKLOADS {
		let bank = synthetic_bank(keys);
		benchmark_editor(size, &bank, keys, &context);
		benchmark_document_limit(size, &bank);
	}
}

fn benchmark_editor(size: &str, bank: &[u8], keys: usize, context: &SigningContext) {
	let open = || Session::open(bank, Limits::default(), None).expect("valid synthetic bank");
	measure(
		"editor_open",
		size,
		bank.len(),
		|| (),
		|_| open(),
		|_, session| {
			assert_eq!(session.document().source(), bank);
			assert_eq!(session.revision(), Revision::INITIAL);
		},
	);

	let initial = open();
	let changes = initial
		.document()
		.sections()
		.iter()
		.flat_map(|section| section.keys())
		.flat_map(|key| key.values())
		.take(BULK_EDITS)
		.map(|value| ValueChange {
			id: initial.id_of_value(value.id()).expect("projected value"),
			value: GROUPED_VALUE,
		})
		.collect::<Vec<_>>();
	assert_eq!(changes.len(), keys.min(BULK_EDITS));
	let apply = |session: &mut Session| {
		session
			.set_values(Revision::INITIAL, &changes)
			.expect("valid synthetic edits")
	};
	measure(
		"editor_bulk_edit",
		size,
		bank.len(),
		open,
		apply,
		|session, revision| {
			assert!(revision > Revision::INITIAL);
			assert!(session.can_undo() && session.is_dirty());
			let values = session.document().sections()[0]
				.keys()
				.iter()
				.flat_map(|key| key.values());
			assert!(
				values.take(changes.len()).all(|value| value
					.attribute()
					.expect("known integer")
					.value()
					== EXACT_VALUE)
			);
		},
	);

	let edited = || {
		let mut session = open();
		assert!(apply(&mut session) > Revision::INITIAL);
		session
	};
	measure(
		"editor_opening_diff",
		size,
		bank.len(),
		edited,
		|session| {
			session
				.opening_diff()
				.expect("difference from opening checkpoint")
		},
		|_, diff| {
			assert!(diff.source_changed);
			assert_eq!(
				diff.changes
					.iter()
					.filter(|change| change.kind == OccurrenceKind::Value)
					.count(),
				changes.len()
			);
		},
	);
	measure(
		"editor_prepare_signed_export",
		size,
		bank.len(),
		edited,
		|session| {
			session
				.prepare(session.revision(), ExportSigning::Sign(context))
				.expect("checked and signed export")
		},
		|session, prepared| {
			assert_eq!(prepared.signature_status(), SignatureStatus::Valid);
			assert_eq!(prepared.revision(), session.revision());
			assert!(prepared.bytes().len() > session.document().source().len());
			assert!(session.is_dirty());
		},
	);
}

fn benchmark_document_limit(size: &str, bank: &[u8]) {
	let limits = Limits {
		document_bytes: bank.len(),
		..Limits::default()
	};
	let mut oversized = bank.to_vec();
	oversized.push(b'\n');
	measure(
		"editor_raw_byte_limit",
		size,
		oversized.len(),
		|| Session::open(bank, limits, None).expect("bank fits exactly"),
		|session| session.replace_raw(Revision::INITIAL, &oversized),
		|session, result| {
			assert!(matches!(
				result,
				Err(EditorError::Parse(error))
					if error.kind() == ParseErrorKind::LimitExceeded(LimitKind::DocumentBytes)
			));
			assert_eq!(session.revision(), Revision::INITIAL);
			assert_eq!(session.document().source(), bank);
			assert!(!session.can_undo() && !session.is_dirty());
		},
	);
}
