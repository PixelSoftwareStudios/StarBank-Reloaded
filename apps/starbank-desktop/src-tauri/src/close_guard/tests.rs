use starbank_core::Revision;

use super::*;

#[test]
fn create_copy_support_offers_save_only_without_an_unapplied_draft() {
	assert!(
		offers_save(false, true),
		"qualified create-copy platforms can save before closing"
	);
	assert!(
		!offers_save(true, true),
		"an unapplied draft must be reviewed first"
	);
	assert!(
		!offers_save(false, false),
		"the renderer can refuse an unavailable action"
	);
	assert!(!offers_save(true, false));
}

#[test]
fn cancelled_close_approval_cannot_be_taken_or_reused() {
	let active = target("bank-1", "4", true);
	let mut guard = Guard::default();
	let probe = guard
		.begin(Intent::Window, Some(active.clone()))
		.expect("close probe");
	guard.attempt(probe.ticket).expect("pending prompt").phase = Phase::Prompt;
	guard.clear(probe.ticket);
	assert!(guard.take_attempt(probe.ticket).is_none());
	let retry = guard
		.begin(Intent::Window, Some(active))
		.expect("fresh close probe");
	assert_ne!(retry.ticket, probe.ticket);
	assert!(
		guard.take_attempt(probe.ticket).is_none(),
		"a stale ticket leaves the retry intact"
	);
	assert!(guard.take_attempt(retry.ticket).is_some());
	assert!(guard.pending.is_none());
}

#[test]
fn a_create_only_save_permits_close_only_after_verified_checkpoint_acknowledgement()
-> Result<(), Box<dyn std::error::Error>> {
	use std::fs;

	use starbank_editor::{Engine, ExportSigning, max_attachment_bytes};
	use starbank_fs::{CommitIntent, CommitOutcome};

	const ORIGINAL: &[u8] = b"<Bank><Section name='Original'/></Bank>";
	const EDITED: &[u8] = b"<Bank><Section name='Edited'/></Bank>";
	const OCCUPANT: &[u8] = b"<Bank><Section name='Independent'/></Bank>";
	const DRAFT_GENERATION: u32 = 7;
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let source = root.join("Original.SC2Bank");
	let occupied = root.join("Occupied.SC2Bank");
	let destination = root.join("Saved.SC2Bank");
	fs::write(&source, ORIGINAL)?;
	fs::write(&occupied, OCCUPANT)?;
	let original = starbank_fs::read_snapshot(&source, max_attachment_bytes())?;
	let mut engine = Engine::default();
	let opening = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind": "open"}
	});
	assert_snapshot(&engine.dispatch(&opening.to_string(), Some(ORIGINAL)));
	edit(&mut engine, EDITED);
	let dirty = engine.native_active_target().expect("dirty document");
	assert!(dirty.dirty);
	let mut guard = Guard::default();
	let first = approved_save(&mut guard, &dirty, DRAFT_GENERATION);
	let rejected = engine.prepare_native_active_save(
		&dirty.document_id,
		dirty.revision,
		ExportSigning::PreserveFor(None),
	)?;
	assert!(matches!(
		starbank_fs::commit(
			&occupied,
			rejected.bytes(),
			CommitIntent::CreateNew,
			max_attachment_bytes()
		),
		CommitOutcome::RejectedUnchanged(_)
	));
	drop(rejected);
	let failed = guard
		.take_attempt(first.ticket)
		.expect("consume failed save approval");
	assert!(!failed.permits(engine.native_active_target().as_ref(), DRAFT_GENERATION));
	assert!(
		guard.take_attempt(first.ticket).is_none(),
		"completion consumes approval after failure"
	);
	assert_eq!(engine.native_active_target().as_ref(), Some(&dirty));
	assert!(!destination.exists());
	assert_eq!(fs::read(&occupied)?, OCCUPANT);

	let retry = approved_save(&mut guard, &dirty, DRAFT_GENERATION);
	assert_ne!(retry.ticket, first.ticket);
	let receipt = engine.prepare_native_active_save(
		&dirty.document_id,
		dirty.revision,
		ExportSigning::PreserveFor(None),
	)?;
	engine.validate_native_active_save(&receipt)?;
	let committed = starbank_fs::commit(
		&destination,
		receipt.bytes(),
		CommitIntent::CreateNew,
		max_attachment_bytes(),
	);
	let CommitOutcome::CommittedVerified(committed) = committed else {
		panic!("expected a verified create-only copy: {committed:?}");
	};
	let saved_file = starbank_fs::read_snapshot(&destination, max_attachment_bytes())?;
	assert_eq!(saved_file.bytes(), receipt.bytes());
	assert_eq!(saved_file.fingerprint(), committed.fingerprint());
	let revision = engine.acknowledge_verified_native_active_save(receipt)?;
	let saved = engine.native_active_target().expect("saved document");
	assert_eq!(saved.document_id, dirty.document_id);
	assert_eq!(saved.revision, revision);
	assert!(!saved.dirty);
	let completion = guard
		.take_attempt(retry.ticket)
		.expect("approved saved completion");
	assert!(completion.permits(Some(&saved), DRAFT_GENERATION));
	assert!(!completion.permits(Some(&saved), DRAFT_GENERATION + 1));
	assert!(!completion.permits(Some(&target("another-bank", "1", false)), DRAFT_GENERATION));
	edit(&mut engine, ORIGINAL);
	assert!(!completion.permits(engine.native_active_target().as_ref(), DRAFT_GENERATION));
	let unchanged = starbank_fs::read_snapshot(&source, max_attachment_bytes())?;
	assert_eq!(unchanged.bytes(), ORIGINAL);
	assert_eq!(unchanged.fingerprint(), original.fingerprint());
	assert_eq!(fs::read(destination)?, EDITED);
	assert_eq!(fs::read(occupied)?, OCCUPANT);
	assert_eq!(
		fs::read_dir(root)?.count(),
		3,
		"no pending or backup artifacts"
	);
	Ok(())
}

fn approved_save(guard: &mut Guard, active: &NativeActiveTarget, generation: u32) -> CloseProbe {
	let probe = guard
		.begin(Intent::Window, Some(active.clone()))
		.expect("save close probe");
	guard
		.attempt(probe.ticket)
		.expect("pending save prompt")
		.phase = Phase::Approved {
		choice: CloseChoice::Save,
		draft_generation: generation,
	};
	probe
}

fn edit(engine: &mut starbank_editor::Engine, bytes: &[u8]) {
	let active = engine.native_active_target().expect("active edit scope");
	let request = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "edit",
		"document_id": active.document_id,
		"expected_revision": active.revision.to_string(),
		"command": {"kind": "replace_raw_text"}
	});
	assert_snapshot(&engine.dispatch(&request.to_string(), Some(bytes)));
}

fn assert_snapshot(reply: &starbank_editor::WireReply) {
	let control: serde_json::Value = serde_json::from_str(&reply.json).expect("engine reply");
	assert_eq!(control["result"]["kind"], "snapshot", "{}", reply.json);
}

fn target(id: &str, revision: &str, dirty: bool) -> NativeActiveTarget {
	NativeActiveTarget {
		document_id: id.to_owned(),
		revision: revision.parse::<Revision>().expect("synthetic revision"),
		dirty,
	}
}

#[test]
fn discard_requires_current_bank_and_draft_generation() {
	let active = target("bank-1", "4", true);
	let mut guard = Guard::default();
	let probe = guard
		.begin(Intent::Window, Some(active.clone()))
		.expect("new probe");
	let attempt = guard.attempt(probe.ticket).expect("pending");
	assert!(!attempt.permits(Some(&active), 7));
	attempt.phase = Phase::Approved {
		choice: CloseChoice::Discard,
		draft_generation: 7,
	};
	assert!(attempt.permits(Some(&active), 7));
	assert!(!attempt.permits(Some(&active), 8));
	assert!(!attempt.permits(Some(&target("bank-1", "5", true)), 7));
	assert!(!attempt.permits(Some(&target("bank-2", "4", true)), 7));
	assert!(!attempt.permits(None, 7));
}

#[test]
fn save_requires_a_clean_checkpoint_after_confirmation() {
	let active = target("bank-1", "4", true);
	let attempt = Attempt {
		ticket: 1,
		intent: Intent::Window,
		captured: Some(active.clone()),
		phase: Phase::Approved {
			choice: CloseChoice::Save,
			draft_generation: 7,
		},
	};
	assert!(!attempt.permits(Some(&active), 7));
	assert!(attempt.permits(Some(&target("bank-1", "5", false)), 7));
	assert!(!attempt.permits(Some(&target("bank-1", "6", true)), 7));
	assert!(!attempt.permits(Some(&target("bank-2", "5", false)), 7));
	assert!(!attempt.permits(Some(&target("bank-1", "5", false)), 8));
}

#[test]
fn no_engine_still_requires_approval_and_an_unchanged_draft() {
	let mut guard = Guard::default();
	let probe = guard.begin(Intent::Window, None).expect("new probe");
	let attempt = guard.attempt(probe.ticket).expect("pending");
	assert!(!attempt.permits(None, 7));
	attempt.phase = Phase::Approved {
		choice: CloseChoice::Cancel,
		draft_generation: 7,
	};
	assert!(!attempt.permits(None, 7));
	attempt.phase = Phase::Approved {
		choice: CloseChoice::Discard,
		draft_generation: 7,
	};
	assert!(attempt.permits(None, 7));
	assert!(!attempt.permits(None, 8));
	assert!(!attempt.permits(Some(&target("late-bank", "0", false)), 7));
	attempt.phase = Phase::Approved {
		choice: CloseChoice::Save,
		draft_generation: 7,
	};
	assert!(!attempt.permits(None, 7));
	assert!(!attempt.permits(Some(&target("late-bank", "0", false)), 7));
}

#[test]
fn retried_probes_cannot_reuse_previous_approvals() {
	let active = target("bank-1", "4", true);
	let mut guard = Guard::default();
	let first = guard.begin(Intent::Window, Some(active.clone())).unwrap();
	let second = guard.begin(Intent::Exit(0), Some(active.clone())).unwrap();
	assert_ne!(first.ticket, second.ticket);
	assert!(guard.attempt(first.ticket).is_none());
	guard.attempt(second.ticket).unwrap().phase = Phase::Prompt;
	assert!(guard.begin(Intent::Window, Some(active)).is_none());
	guard.clear(first.ticket);
	assert!(guard.attempt(second.ticket).is_some());
	guard.clear(second.ticket);
	assert!(guard.pending.is_none());
}
