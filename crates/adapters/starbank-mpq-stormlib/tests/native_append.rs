#![cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]

use std::fs;
use std::path::Path;

use starbank_mpq::{V1Addition, V1AdditionIssue, V1AppendError, V1AppendPlan};
use starbank_mpq_stormlib::append;

const FIXTURE: &str = "../starbank-mpq/tests/fixtures/v1-no-list.s2ma";
const ANNOTATION: &[u8] = b"Synthetic archive annotation\n";
const OTHER_COMPONENT: &[u8] = b"Second synthetic archive member";

#[test]
fn appends_and_reopens_synthetic_candidate_without_touching_source() {
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
	let before = fs::read(&fixture).expect("synthetic source bytes");
	let additions = [
		V1Addition {
			name: "StarBankProbe.txt",
			bytes: ANNOTATION,
		},
		V1Addition {
			name: "TestAssets\\Preserve.bin",
			bytes: OTHER_COMPONENT,
		},
	];
	let plan = V1AppendPlan::prepare(&before, &additions).expect("qualified synthetic archive");
	let candidate = append(&plan).expect("create verified append candidate");
	assert_ne!(&candidate, &before);
	plan.verify(&candidate)
		.expect("independent parser verifies retained and appended members");
	assert_eq!(fs::read(&fixture).expect("read original again"), before);

	let rerun = V1AppendPlan::prepare(&candidate, &additions);
	assert!(matches!(
		rerun,
		Err(V1AppendError::Addition(V1AdditionIssue::ExistingMember))
	));
}

#[test]
fn stores_unaligned_patch_like_and_single_byte_payloads_as_exact_ordinary_members() {
	const C_WORD_ALIGNMENT: usize = 4;
	const PAYLOAD_BYTES: usize = 8192;
	// BSD0 at byte 64 is StormLib 9.40's incremental-patch detection field.
	// These are authored opaque bytes, not an executable or an SC2 component.
	const PATCH_TYPE_OFFSET: usize = 64;
	let mut storage = vec![0; PAYLOAD_BYTES + C_WORD_ALIGNMENT];
	let offset =
		(C_WORD_ALIGNMENT + 1 - storage.as_ptr().addr() % C_WORD_ALIGNMENT) % C_WORD_ALIGNMENT;
	let payload = &mut storage[offset..offset + PAYLOAD_BYTES];
	payload[PATCH_TYPE_OFFSET..PATCH_TYPE_OFFSET + 4].copy_from_slice(b"BSD0");
	assert_eq!(payload.as_ptr().addr() % C_WORD_ALIGNMENT, 1);
	let source = include_bytes!("../../starbank-mpq/tests/fixtures/v1-no-list.s2ma");
	let additions = [
		V1Addition {
			name: "OpaqueUnaligned.bin",
			bytes: payload,
		},
		V1Addition {
			name: "SingleByte.bin",
			bytes: b"x",
		},
	];
	let plan = V1AppendPlan::prepare(source, &additions).expect("qualified synthetic source");
	let candidate = append(&plan).expect("store exact opaque payloads");
	plan.verify(&candidate)
		.expect("ordinary flags and exact bytes");
}
