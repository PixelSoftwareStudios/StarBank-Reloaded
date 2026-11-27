use std::{fs, io};

use super::{InventoryError, InventoryReason, InventoryService, InventoryStatus, ScanMode};

const CORRUPT: &[u8] = b"{not a valid root record";

#[test]
fn unreadable_root_blocks_passive_scans_and_survives_cancelled_recovery() -> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let config = root.join("inventory.json");
	fs::write(&config, CORRUPT)?;
	let inventory = InventoryService::default().with_root_record(Some(config.clone()));
	for mode in [ScanMode::Refresh, ScanMode::RebuildMapIndex] {
		let snapshot = inventory
			.scan_with_progress(mode, &[], |_| panic!("blocked scans must not start"))
			.expect("configuration failure snapshot");
		assert_eq!(snapshot.status, InventoryStatus::Failed);
		assert_eq!(
			snapshot.reason,
			Some(InventoryReason::ConfigurationUnavailable)
		);
		assert!(snapshot.banks.is_empty());
		assert!(snapshot.maps.is_empty());
		assert!(!snapshot.can_rebuild_map_index);
	}
	let aborted = inventory.scan_with_progress(ScanMode::Automatic, &[], |event| {
		let event = serde_json::to_value(event).expect("progress JSON");
		let generation = event["generation"].as_str().expect("progress generation");
		inventory
			.cancel(generation.parse().expect("numeric generation"))
			.expect("cancel explicit detection before root lookup");
	});
	assert!(matches!(aborted, Err(InventoryError::Cancelled)));
	assert_eq!(fs::read(&config)?, CORRUPT);
	let snapshot = inventory
		.scan_with_roots(ScanMode::Refresh, &[])
		.expect("still blocked");
	assert_eq!(
		snapshot.reason,
		Some(InventoryReason::ConfigurationUnavailable)
	);
	Ok(())
}

#[test]
fn unreadable_root_is_replaced_after_a_verified_manual_selection() -> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let config = root.join("inventory.json");
	fs::write(&config, CORRUPT)?;
	let inventory = InventoryService::default().with_root_record(Some(config.clone()));

	let invalid = inventory
		.scan_with_roots(ScanMode::Manual(root.join("Absent")), &[])
		.expect("invalid picker result");
	assert_eq!(
		invalid.reason,
		Some(InventoryReason::ConfigurationUnavailable)
	);
	assert_eq!(fs::read(&config)?, CORRUPT);
	assert!(invalid.banks.is_empty());

	let selected = root.join("Explicit");
	fs::create_dir(&selected)?;
	fs::write(selected.join("Synthetic.SC2Bank"), b"<Bank/>")?;
	let recovered = inventory
		.scan_with_roots(ScanMode::Manual(selected.clone()), &[])
		.expect("verified manual recovery");
	assert_eq!(recovered.status, InventoryStatus::Ready);
	assert_eq!(recovered.reason, None);
	assert_eq!(recovered.banks.len(), 1);
	assert_ne!(fs::read(&config)?, CORRUPT);

	let restarted = InventoryService::default().with_root_record(Some(config));
	let restored = restarted
		.scan_with_roots(ScanMode::Refresh, &[])
		.expect("recovered scope persisted");
	assert_eq!(restored.status, InventoryStatus::Ready);
	assert!(restored.manual);
	assert_eq!(restored.root_name.as_deref(), Some("Explicit"));
	assert_eq!(restored.banks.len(), 1);
	Ok(())
}

#[test]
fn auto_recovery_replaces_corrupt_configuration_but_refuses_a_valid_replacement() -> io::Result<()>
{
	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let config = root.join("inventory.json");
	fs::write(&config, CORRUPT)?;
	let store = super::RootRecordStore::new(config.clone());
	store.recover(None)?;
	assert!(store.load()?.is_none());
	let recovered = fs::read(&config)?;
	assert_ne!(recovered, CORRUPT);

	let selected = root.join("Confirmed");
	fs::create_dir(&selected)?;
	assert_eq!(
		store
			.recover(Some(&super::ManualRoot {
				identity: Some(
					super::authority::DirectoryIdentity::read(&selected).expect("folder identity")
				),
				path: selected,
			}))
			.expect_err("refuse a concurrent valid root selection")
			.kind(),
		io::ErrorKind::ResourceBusy,
	);
	assert_eq!(fs::read(&config)?, recovered);
	Ok(())
}

#[test]
fn unresolved_config_path_requires_an_explicit_scope_in_each_session() -> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	fs::write(root.join("Synthetic.SC2Bank"), b"<Bank/>")?;
	let inventory = InventoryService::default().with_root_record(None);
	assert_passive_scans_are_blocked(&inventory);
	let invalid = inventory
		.scan_with_roots(ScanMode::Manual(root.join("Absent")), &[])
		.expect("failed explicit selection");
	assert_eq!(invalid.status, InventoryStatus::Failed);
	assert!(invalid.banks.is_empty());
	assert_passive_scans_are_blocked(&inventory);

	let confirmed = inventory
		.scan_with_roots(ScanMode::Manual(root.clone()), &[])
		.expect("explicit in-session manual authority");
	assert_eq!(confirmed.status, InventoryStatus::Ready);
	assert_eq!(
		confirmed.reason,
		Some(InventoryReason::ConfigurationUnavailable)
	);
	assert!(confirmed.manual && confirmed.incomplete);
	assert_eq!(confirmed.banks.len(), 1);
	let refreshed = inventory
		.scan_with_roots(ScanMode::Refresh, &[])
		.expect("refresh retained in-session authority");
	assert_eq!(refreshed.status, InventoryStatus::Ready);
	assert_eq!(refreshed.reason, confirmed.reason);
	assert_eq!(refreshed.banks.len(), 1);
	assert_passive_scans_are_blocked(&InventoryService::default().with_root_record(None));

	let in_memory = InventoryService::default()
		.scan_with_roots(ScanMode::Manual(root.clone()), &[])
		.expect("headless test service without persistence");
	assert_eq!(in_memory.reason, None);
	let config = root.join("config").join("inventory.json");
	let fresh = InventoryService::default()
		.with_root_record(Some(config.clone()))
		.scan_with_roots(ScanMode::Manual(root), &[])
		.expect("new record at a known config path");
	assert_eq!(fresh.reason, None);
	assert!(config.is_file());
	Ok(())
}

fn assert_passive_scans_are_blocked(inventory: &InventoryService) {
	for mode in [ScanMode::Refresh, ScanMode::RebuildMapIndex] {
		let snapshot = inventory
			.scan_with_progress(mode, &[], |_| {
				panic!("unavailable config cannot authorize a scan")
			})
			.expect("configuration failure snapshot");
		assert_eq!(snapshot.status, InventoryStatus::Failed);
		assert_eq!(
			snapshot.reason,
			Some(InventoryReason::ConfigurationUnavailable)
		);
		assert!(snapshot.incomplete);
		assert!(snapshot.banks.is_empty() && snapshot.maps.is_empty());
	}
}
