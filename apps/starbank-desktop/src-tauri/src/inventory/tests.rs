use std::{
	fs, io,
	path::{Path, PathBuf},
};

use serde_json::Value;

use super::*;

const ALPHA: &[u8] =
	include_bytes!("../../../../../crates/discovery/starbank-map-index/tests/fixtures/alpha.s2ma");
const RIVAL: &[u8] =
	include_bytes!("../../../../../crates/discovery/starbank-map-index/tests/fixtures/rival.s2ma");
const EMPTY: &[u8] =
	include_bytes!("../../../../../crates/discovery/starbank-map-index/tests/fixtures/empty.s2ma");

fn write(root: &Path, relative: &str, bytes: &[u8]) -> io::Result<PathBuf> {
	let path = root.join(relative);
	let parent = path
		.parent()
		.ok_or_else(|| io::Error::other("fixture lacks parent"))?;
	fs::create_dir_all(parent)?;
	fs::write(&path, bytes)?;
	Ok(path)
}

fn serialize_snapshot(snapshot: InventorySnapshot) -> io::Result<Value> {
	serde_json::to_value(snapshot).map_err(io::Error::other)
}

fn manual_snapshot(service: &InventoryService, root: &Path, maps: &[PathBuf]) -> io::Result<Value> {
	let snapshot = service
		.scan_with_roots(ScanMode::Manual(root.to_owned()), maps)
		.map_err(|error| io::Error::other(format!("scan failed: {error:?}")))?;
	serialize_snapshot(snapshot)
}

#[test]
fn manual_inventory_projects_collisions_and_unknowns_using_only_session_ids() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let game = root.join("Private SC2");
	let player_a = write(
		&game,
		"Accounts/12/1-S2-1-01/Banks/1-S2-1-44/Shared.SC2Bank",
		b"<Bank/>",
	)?;
	write(
		&game,
		"Accounts/22/2-S2-1-02/Banks/2-S2-1-55/shared.SC2Bank",
		b"<Bank/>",
	)?;
	write(
		&game,
		"Accounts/22/2-S2-1-02/Banks/2-S2-1-55/unmapped.SC2Bank",
		b"<Bank/>",
	)?;
	write(
		&game,
		"Accounts/12/1-S2-1-01/Banks/1-S2-1-44/backup.sc2bank.bak",
		b"not live",
	)?;
	let maps = root.join("Cached");
	write(&maps, "alpha.s2ma", ALPHA)?;
	write(&maps, "rival.s2ma", RIVAL)?;
	write(&maps, "empty.s2ma", EMPTY)?;
	let map_roots = [maps];
	let service = InventoryService::with_cache_path(Some(root.join("map-metadata.json")));

	let snapshot = manual_snapshot(&service, &game, &map_roots)?;
	assert_eq!(snapshot["status"], "ready");
	assert_eq!(snapshot["rootName"], "Private SC2");
	assert_eq!(snapshot["manual"], true);
	assert_eq!(snapshot["incomplete"], false);
	let listed_banks = snapshot["banks"].as_array().expect("bank DTOs");
	assert_eq!(listed_banks.len(), 3);
	let listed_maps = snapshot["maps"].as_array().expect("map DTOs");
	assert_eq!(listed_maps.len(), 3);
	let matched = listed_banks
		.iter()
		.find(|bank| bank["playerHandle"] == "1-S2-1-01")
		.expect("bank A");
	assert_eq!(matched["authorHandle"], "1-S2-1-44");
	let candidate_ids = matched["mapIds"].as_array().expect("candidate IDs");
	assert_eq!(candidate_ids.len(), 2);
	assert!(candidate_ids.iter().all(|id| {
		listed_maps
			.iter()
			.any(|map| map["id"] == *id && map["hasBankFiles"] == true)
	}));
	assert_eq!(
		listed_maps
			.iter()
			.filter(|map| map["hasBankFiles"] == false)
			.count(),
		1
	);
	assert!(
		listed_banks
			.iter()
			.any(|bank| bank["name"] == "unmapped.SC2Bank"
				&& bank["mapIds"] == serde_json::json!([]))
	);
	assert!(
		!snapshot
			.to_string()
			.contains(root.to_string_lossy().as_ref())
	);

	let id = matched["id"].as_str().expect("bank ID");
	assert_eq!(
		service.authorize_bank(id).expect("authorized bank").path(),
		player_a
	);
	let refreshed = service
		.scan_with_roots(ScanMode::Refresh, &map_roots)
		.expect("refresh");
	assert_eq!(serialize_snapshot(refreshed)?, snapshot);
	assert!(matches!(
		service.authorize_bank(player_a.to_string_lossy().as_ref()),
		Err(InventoryError::UnknownBank)
	));
	assert!(matches!(
		service.authorize_bank("bank-0000000000000000"),
		Err(InventoryError::UnknownBank)
	));
	let second_root = root.join("Second manual folder");
	fs::create_dir(&second_root)?;
	let empty = manual_snapshot(&service, &second_root, &map_roots)?;
	assert_eq!(empty["banks"], serde_json::json!([]));
	assert!(matches!(
		service.authorize_bank(id),
		Err(InventoryError::UnknownBank)
	));
	Ok(())
}

#[test]
fn a_cancellation_releases_the_scan_lease_without_executing_another_scan() -> io::Result<()> {
	let service = InventoryService::default();
	let lease = service.start_scan(ScanMode::Refresh).expect("first lease");
	assert!(matches!(
		service.start_scan(ScanMode::Refresh),
		Err(InventoryError::Busy)
	));
	service
		.cancel(lease.generation - 1)
		.expect("ignore stale cancel");
	assert!(!lease.cancellation.is_cancelled());
	service
		.cancel(lease.generation)
		.expect("cancel current scan");
	assert!(lease.cancellation.is_cancelled());
	drop(lease);
	let next = service
		.start_scan(ScanMode::Refresh)
		.expect("released lease");
	assert!(!next.cancellation.is_cancelled());
	Ok(())
}

#[test]
fn a_stale_manual_root_failure_keeps_the_override_and_redacts_the_path() -> io::Result<()> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?.join("Former collection");
	fs::create_dir(&root)?;
	let service = InventoryService::default();
	let initial = manual_snapshot(&service, &root, &[])?;
	assert_eq!(initial["status"], "ready");
	fs::remove_dir(&root)?;
	let snapshot = service
		.scan_with_roots(ScanMode::Refresh, &[])
		.expect("typed failure");
	let failure = serialize_snapshot(snapshot)?;
	assert_eq!(failure["status"], "failed");
	assert_eq!(failure["reason"], "invalid_folder");
	assert_eq!(failure["rootName"], "Former collection");
	assert_eq!(failure["manual"], true);
	assert!(
		!failure
			.to_string()
			.contains(temp.path().to_string_lossy().as_ref())
	);
	Ok(())
}

#[cfg(unix)]
#[test]
fn stale_bank_id_cannot_follow_replaced_directory_or_bank_symlink() -> io::Result<()> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let selected = root.join("Selected");
	let path = write(&selected, "nested/Allowed.sc2bank", b"<Bank/>")?;
	let outsider = write(&root, "Outside/Secret.sc2bank", b"<Bank/>")?;
	let service = InventoryService::default();
	let snapshot = manual_snapshot(&service, &selected, &[])?;
	let id = snapshot["banks"][0]["id"].as_str().expect("bank id");
	assert!(service.authorize_bank(id).is_ok());
	fs::remove_file(&path)?;
	std::os::unix::fs::symlink(&outsider, &path)?;
	assert!(matches!(
		service.authorize_bank(id),
		Err(InventoryError::StaleBank)
	));

	fs::rename(&selected, root.join("Old folder"))?;
	write(&selected, "nested/Allowed.sc2bank", b"<Bank/>")?;
	assert!(matches!(
		service.authorize_bank(id),
		Err(InventoryError::StaleBank)
	));
	Ok(())
}

#[test]
fn picker_authorized_root_persists_and_refuses_replaced_directory_after_restart() -> io::Result<()>
{
	let sandbox = tempfile::tempdir()?;
	let root = sandbox.path().canonicalize()?;
	let config = root.join("App config").join("inventory-root.json");
	let selected = root.join("Authoritative collection");
	write(&selected, "Saved.SC2Bank", b"<Bank/>")?;
	let original = InventoryService::default().with_root_record(Some(config.clone()));
	let before = manual_snapshot(&original, &selected, &[])?;
	assert_eq!(before["banks"][0]["name"], "Saved.SC2Bank");
	assert_eq!(before["incomplete"], false);
	assert!(config.is_file());

	let restarted = InventoryService::default().with_root_record(Some(config.clone()));
	let after = restarted
		.scan_with_roots(ScanMode::Refresh, &[])
		.expect("restored scan");
	let restored = serialize_snapshot(after)?;
	assert_eq!(restored["status"], "ready");
	assert_eq!(restored["manual"], true);
	assert_eq!(restored["banks"][0]["name"], "Saved.SC2Bank");
	assert!(
		!restored
			.to_string()
			.contains(root.to_string_lossy().as_ref())
	);

	#[cfg(unix)]
	{
		fs::rename(&selected, root.join("Former authorized collection"))?;
		write(&selected, "Never authorized.SC2Bank", b"<Bank/>")?;
		let stale = InventoryService::default().with_root_record(Some(config.clone()));
		let denied = stale
			.scan_with_roots(ScanMode::Refresh, &[])
			.expect("redacted stale state");
		let denied = serialize_snapshot(denied)?;
		assert_eq!(denied["status"], "failed");
		assert_eq!(denied["reason"], "invalid_folder");
		assert_eq!(denied["manual"], true);
		assert_eq!(denied["banks"], serde_json::json!([]));
	}

	RootRecordStore::new(config.clone()).save(None)?;
	let cleared = InventoryService::default().with_root_record(Some(config));
	let automatic = cleared
		.start_scan(ScanMode::Refresh)
		.expect("automatic state");
	assert!(automatic.root.is_none());
	Ok(())
}

#[test]
fn legacy_folder_hint_does_not_authorize_a_scan() -> io::Result<()> {
	let sandbox = tempfile::tempdir()?;
	let root = sandbox
		.path()
		.canonicalize()?
		.join("Original settings hint");
	write(&root, "Unconfirmed.SC2Bank", b"<Bank/>")?;
	let service = InventoryService::default().with_starting_directory(Some(root.clone()));
	assert_eq!(
		service.picker_directory().expect("dialog default"),
		Some(root)
	);
	let scan = service
		.start_scan(ScanMode::Refresh)
		.expect("automatic scan lease");
	assert!(scan.root.is_none());
	Ok(())
}

#[test]
fn map_index_activity_emits_actual_archive_counts_and_honors_cancel_boundary() -> io::Result<()> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let bank_root = root.join("Selected bank fixture");
	write(&bank_root, "Shared.sc2bank", b"<Bank/>")?;
	let map_root = root.join("Archives");
	write(&map_root, "first.s2ma", ALPHA)?;
	write(&map_root, "second.s2ma", RIVAL)?;
	let roots = [map_root];
	let service = InventoryService::default();
	let mut events = Vec::new();
	service
		.scan_with_progress(ScanMode::Manual(bank_root.clone()), &roots, |event| {
			events.push(serde_json::to_value(event).expect("progress JSON"));
		})
		.expect("two observed archives");
	assert_eq!(events[0]["stage"], "locating_folder");
	assert_eq!(events[1]["stage"], "enumerating_banks");
	assert_eq!(events[2]["stage"], "indexing_maps");
	assert_eq!(
		events[3],
		serde_json::json!({"generation": "1", "stage": "inspecting_archives", "completed": 1, "total": 2})
	);
	assert_eq!(
		events[4],
		serde_json::json!({"generation": "1", "stage": "inspecting_archives", "completed": 2, "total": 2})
	);
	assert_eq!(events.len(), 5);

	let cancelled = service.scan_with_progress(ScanMode::Refresh, &roots, |event| {
		let message = serde_json::to_value(event).expect("progress JSON");
		assert_eq!(message["generation"], "2");
		if message["stage"] == "indexing_maps" {
			service.cancel(2).expect("request cancellation");
		}
	});
	assert!(matches!(cancelled, Err(InventoryError::Cancelled)));
	assert!(service.start_scan(ScanMode::Refresh).is_ok());
	Ok(())
}

#[test]
fn explicit_map_index_rebuild_uses_the_same_bank_authority_and_cancel_boundary() -> io::Result<()> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let bank_root = root.join("Selected banks");
	let bank = write(&bank_root, "Shared.sc2bank", b"<Bank/>")?;
	let map_root = root.join("Archives");
	let archive = write(&map_root, "map.s2ma", ALPHA)?;
	let roots = [map_root];
	let cache = root.join("map-metadata.json");
	let service = InventoryService::with_cache_path(Some(cache.clone()));
	let initial = manual_snapshot(&service, &bank_root, &roots)?;
	assert_eq!(initial["canRebuildMapIndex"], true);
	let bank_id = initial["banks"][0]["id"].as_str().expect("bank id");
	let previous_cache = fs::read(&cache)?;
	fs::write(&archive, RIVAL)?;

	let cancelled = service.scan_with_progress(ScanMode::RebuildMapIndex, &roots, |event| {
		let message = serde_json::to_value(event).expect("progress JSON");
		if message["stage"] == "inspecting_archives" {
			assert_eq!(message["generation"], "2");
			assert_eq!(message["completed"], 1);
			service.cancel(2).expect("cancel rebuild");
		}
	});
	assert!(matches!(cancelled, Err(InventoryError::Cancelled)));
	assert_eq!(fs::read(&cache)?, previous_cache);
	assert_eq!(
		service
			.authorize_bank(bank_id)
			.expect("existing bank")
			.path(),
		bank
	);

	let rebuilt = service
		.scan_with_roots(ScanMode::RebuildMapIndex, &roots)
		.expect("rebuild after cancellation");
	let rebuilt = serialize_snapshot(rebuilt)?;
	assert_eq!(rebuilt["maps"][0]["name"], "A Different Map");
	assert_eq!(rebuilt["banks"], initial["banks"]);
	assert_ne!(fs::read(&cache)?, previous_cache);
	Ok(())
}
