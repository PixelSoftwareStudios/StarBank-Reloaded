use super::*;

#[test]
fn only_one_batch_lease_runs_and_cancellation_does_not_leak_to_next_batch() {
	let service = BatchBackupService::default();
	let first = service.begin().expect("first batch");
	assert!(matches!(service.begin(), Err(BatchBackupError::Busy)));
	assert!(!first.cancellation.load(Ordering::Acquire));
	service.cancel().expect("signal cancellation");
	assert!(first.cancellation.load(Ordering::Acquire));
	drop(first);
	let next = service.begin().expect("fresh batch after release");
	assert!(!next.cancellation.load(Ordering::Acquire));
}

#[test]
fn renderer_can_select_inventory_ids_and_fixed_destinations_only() {
	assert!(
		serde_json::from_str::<BatchBackupScope>(r#"{"kind":"map","id":"map-1234567812345678"}"#)
			.is_ok()
	);
	assert!(serde_json::from_str::<BatchBackupScope>(r#"{"kind":"all"}"#).is_ok());
	assert!(
		serde_json::from_str::<BatchBackupScope>(
			r#"{"kind":"map","id":"map-1234567812345678","path":"/unsafe"}"#
		)
		.is_err()
	);
	assert!(
		serde_json::from_str::<BatchBackupScope>(r#"{"kind":"all","path":"/unsafe"}"#).is_err()
	);
	assert!(serde_json::from_str::<BatchBackupPlacement>(r#""/unsafe""#).is_err());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn reports_current_member_effect_and_stops_before_starting_another_after_cancel()
-> std::io::Result<()> {
	use std::{fs, num::NonZeroUsize};

	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let managed = root.join("managed");
	fs::create_dir(&managed)?;
	let live = b"<Bank version='1'/>\r\n";
	for name in ["One.SC2Bank", "Two.SC2Bank", "Three.SC2Bank"] {
		fs::write(root.join(name), live)?;
	}
	let plan = crate::inventory::scan_batch_fixture(&root).expect("frozen inventory");
	let cancellation = AtomicBool::new(false);
	let mut expected_first = String::new();
	let mut catalog = NativeBackupCatalog::new(NonZeroUsize::new(1_024).expect("limit"));
	let report = execute(
		plan.members,
		|| {
			cancellation
				.load(Ordering::Acquire)
				.then_some(BatchOutcome::Cancelled)
		},
		|bank, directories| {
			expected_first.clone_from(&bank.id);
			bank.authority.validate().expect("same original membership");
			let report = create_one(
				&mut catalog,
				bank,
				BackupPlacement::Managed(&managed),
				directories,
			);
			cancellation.store(true, Ordering::Release);
			report
		},
	)
	.report;
	assert_eq!((report.created, report.failed, report.skipped), (1, 0, 2));
	assert_eq!(report.entries[0].bank_id, expected_first);
	assert_eq!(report.entries[0].outcome, BatchOutcome::Created);
	assert!(
		report.entries[1..]
			.iter()
			.all(|entry| entry.outcome == BatchOutcome::Cancelled)
	);
	assert_eq!(
		fs::read_dir(&managed)?.count(),
		2,
		"one snapshot and its sidecar"
	);
	let snapshot = report.entries[0]
		.backup_name
		.as_ref()
		.expect("snapshot leaf name");
	assert_eq!(fs::read(managed.join(snapshot))?, live);
	for name in ["One.SC2Bank", "Two.SC2Bank", "Three.SC2Bank"] {
		assert_eq!(fs::read(root.join(name))?, live);
	}
	let wire = serde_json::to_string(&report).expect("report projection");
	assert!(!wire.contains(root.to_string_lossy().as_ref()));
	Ok(())
}

#[test]
fn review_target_only_accepts_a_nonzero_run_and_integer_row() {
	let valid = r#"{"runId":1,"rowIndex":0}"#;
	let target: BatchReviewTarget = serde_json::from_str(valid).expect("valid review target");
	assert_eq!(target.run_id.get(), 1);
	assert_eq!(target.row_index, 0);
	for invalid in [
		r#"{"runId":0,"rowIndex":0}"#,
		r#"{"runId":1,"rowIndex":-1}"#,
		r#"{"runId":1,"rowIndex":0.5}"#,
		r#"{"runId":1,"rowIndex":0,"path":"/unsafe"}"#,
	] {
		assert!(serde_json::from_str::<BatchReviewTarget>(invalid).is_err());
	}
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn completed_review_keeps_evidence_after_cancellation_and_expires_at_next_execution()
-> std::io::Result<()> {
	use std::fs;

	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let live = b"<Bank/>\r\n";
	for name in ["One.SC2Bank", "Two.SC2Bank"] {
		fs::write(root.join(name), live)?;
	}
	let plan = crate::inventory::scan_batch_fixture(&root).expect("inventory");
	let recorded_backup = root.join("One_SC2Bank.sc2bank.bak");
	let recorded_artifact = root.join(".starbank-pending-test");

	let service = BatchBackupService::default();
	let lease = service.begin().expect("batch lease");
	let executed = execute(
		plan.members,
		|| {
			lease
				.cancellation
				.load(Ordering::Acquire)
				.then_some(BatchOutcome::Cancelled)
		},
		|_, directories| {
			lease.cancellation.store(true, Ordering::Release);
			EntryResult::needs_review(
				ReviewKind::SnapshotCommitUncertain,
				&recorded_backup,
				Some(&recorded_artifact),
				&"Synthetic backup verification failure",
				directories,
			)
		},
	);
	let report = lease
		.complete(String::from("main"), executed)
		.expect("completed report");
	assert_eq!((report.created, report.failed, report.skipped), (0, 1, 1));
	assert_eq!(report.entries[0].outcome, BatchOutcome::NeedsReview);
	assert_eq!(report.entries[1].outcome, BatchOutcome::Cancelled);
	let run_id = report.run_id.expect("review run");
	let target = BatchReviewTarget {
		run_id,
		row_index: 0,
	};
	let summary = serde_json::to_string(&report).expect("serialize batch summary");
	assert!(!summary.contains(root.to_string_lossy().as_ref()));
	assert!(!summary.contains("verification failure"));
	let review = serde_json::to_value(service.review("main", target).expect("review row"))
		.expect("serialize a detail");
	assert_eq!(review["runId"], run_id.get());
	assert_eq!(review["rowIndex"], 0);
	assert_eq!(review["kind"], "snapshot_commit_uncertain");
	assert_eq!(
		review["backup"]["display"],
		recorded_backup.to_string_lossy().as_ref()
	);
	assert_eq!(
		review["recovery"]["display"],
		recorded_artifact.to_string_lossy().as_ref()
	);
	assert_eq!(review["reason"], "Synthetic backup verification failure");
	assert_eq!(review["reasonTruncated"], false);
	assert!(matches!(
		service.review("other", target),
		Err(BatchBackupError::InvalidReview)
	));
	for row_index in [1, MAX_BATCH_BANKS] {
		assert!(matches!(
			service.review("main", BatchReviewTarget { run_id, row_index }),
			Err(BatchBackupError::InvalidReview)
		));
	}
	assert_eq!(
		fs::read_dir(&root)?.count(),
		2,
		"review did not create files"
	);
	assert!(!recorded_artifact.exists());

	let planning = service.begin().expect("plan a new run");
	drop(planning);
	assert!(service.review("main", target).is_ok());

	let next = service.begin().expect("next batch lease");
	let plan = crate::inventory::scan_batch_fixture(&root).expect("next inventory");
	let executed = execute(
		plan.members,
		|| None,
		|_, _| EntryResult::plain(BatchOutcome::Rejected),
	);
	let next = next
		.complete(String::from("main"), executed)
		.expect("completed report");
	assert!(next.run_id.is_none());
	assert_eq!(next.failed, 2);
	assert!(matches!(
		service.review("main", target),
		Err(BatchBackupError::InvalidReview)
	));
	Ok(())
}

#[test]
fn rejected_metadata_has_review_details_without_an_artifact_claim()
-> Result<(), Box<dyn std::error::Error>> {
	use starbank_backup::{BackupIntegrity, BackupReason, BackupRecord, BackupRequest};
	use starbank_fs::StorageError;

	let fixture = tempfile::tempdir()?;
	let path = fixture.path().canonicalize()?.join("Live.SC2Bank");
	let source = backup_source(&path)?;
	let request = BackupRequest::adjacent("20261127_000000".parse()?);
	let record = BackupRecord::from_metadata(
		source,
		request,
		0,
		BackupReason::Requested,
		BackupIntegrity::measure(b"<Bank/>"),
	)?;
	let backup = record.id().path().to_owned();
	let mut directories = SharedDirectories::new();
	let outcome = review_policy_error(
		BackupError::Metadata {
			record: Box::new(record),
			failure: RecordFailure::Rejected(StorageError::Conflict),
		},
		&mut directories,
	);
	assert_eq!(outcome.outcome, BatchOutcome::NeedsReview);
	let run_id = NonZeroU32::MAX;
	let review = CompletedReview {
		run_id,
		owner: String::from("main"),
		rows: ReviewRows::from([(0, outcome.review.expect("review evidence"))]),
	};
	let target = BatchReviewTarget {
		run_id,
		row_index: 0,
	};
	let encoded = serde_json::to_value(review.read("main", target).expect("one detail"))?;
	assert_eq!(encoded["kind"], "metadata_rejected");
	assert_eq!(
		encoded["backup"]["display"],
		backup.to_string_lossy().as_ref()
	);
	assert!(encoded["recovery"].is_null());
	assert_eq!(encoded["reason"], StorageError::Conflict.to_string());
	Ok(())
}
