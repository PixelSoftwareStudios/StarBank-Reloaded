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
		|bank| {
			expected_first.clone_from(&bank.id);
			bank.authority.validate().expect("same original membership");
			let report = create_one(&mut catalog, bank, BackupPlacement::Managed(&managed));
			cancellation.store(true, Ordering::Release);
			report
		},
	);
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
