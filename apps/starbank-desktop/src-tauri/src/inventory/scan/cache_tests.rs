use std::fs;

use crate::inventory::{InventoryService, ScanMode};

use super::*;

const ALPHA: &[u8] = include_bytes!(
	"../../../../../../crates/discovery/starbank-map-index/tests/fixtures/alpha.s2ma"
);
const RIVAL: &[u8] = include_bytes!(
	"../../../../../../crates/discovery/starbank-map-index/tests/fixtures/rival.s2ma"
);

fn stage(
	lease: &ScanLease<'_>,
	roots: &[PathBuf],
	cache: MapCache<'_>,
) -> Result<CollectedInventory, InventoryError> {
	let mut progress = InventoryProgressPublisher::new(lease.generation, |_| {});
	collect_inventory(
		lease.root.as_ref(),
		roots,
		MapScanOptions {
			cache,
			max_workers: None,
		},
		&lease.ids,
		lease.generation,
		&lease.cancellation,
		&mut progress,
	)
}

#[test]
fn cancelling_after_refresh_staging_preserves_the_cache_and_published_inventory()
-> std::io::Result<()> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let banks = root.join("Banks");
	let maps = root.join("Maps");
	let cache_path = root.join("metadata.json");
	fs::create_dir(&banks)?;
	fs::create_dir(&maps)?;
	fs::write(banks.join("Shared.SC2Bank"), b"<Bank/>")?;
	fs::write(maps.join("alpha.s2ma"), ALPHA)?;

	let service = InventoryService::with_cache_path(Some(cache_path.clone()));
	let roots = [maps];
	service
		.scan_with_roots(ScanMode::Manual(banks), &roots)
		.expect("publish the initial map and bank");
	let previous_cache = fs::read(&cache_path)?;
	let previous_maps = service.state.lock().unwrap().map_members.clone();
	assert_eq!(previous_maps.len(), 1);
	fs::write(roots[0].join("rival.s2ma"), RIVAL)?;

	let lease = service
		.start_scan(ScanMode::Refresh)
		.expect("start refresh");
	let policy = MapCache::Reuse(Some(&cache_path));
	let collected = stage(&lease, &roots, policy).expect("stage refresh");
	assert!(
		collected.2.is_some(),
		"new cache staged before cancellation"
	);
	assert_eq!(fs::read(&cache_path)?, previous_cache);
	service
		.cancel(lease.generation)
		.expect("cancel staged refresh");
	assert!(matches!(
		lease.publish(policy, collected),
		Err(InventoryError::Cancelled)
	));
	assert_eq!(fs::read(&cache_path)?, previous_cache);
	assert_eq!(service.state.lock().unwrap().map_members, previous_maps);

	service
		.scan_with_roots(ScanMode::Refresh, &roots)
		.expect("retry refresh");
	assert_ne!(fs::read(cache_path)?, previous_cache);
	assert_eq!(service.state.lock().unwrap().map_members.len(), 2);
	Ok(())
}

#[test]
fn failed_refresh_cache_publication_warns_without_rejecting_the_inventory() -> std::io::Result<()> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let banks = root.join("Banks");
	let maps = root.join("Maps");
	let cache_path = root.join("metadata.json");
	fs::create_dir(&banks)?;
	fs::create_dir(&maps)?;
	fs::write(maps.join("alpha.s2ma"), ALPHA)?;

	let service = InventoryService::with_cache_path(Some(cache_path.clone()));
	let lease = service
		.start_scan(ScanMode::Manual(banks))
		.expect("start refresh");
	let policy = MapCache::Reuse(Some(&cache_path));
	let collected = stage(&lease, &[maps], policy).expect("stage refresh");
	let ((snapshot, _), _, update) = &collected;
	assert!(update.is_some());
	assert!(!snapshot.incomplete);
	fs::create_dir(&cache_path)?;

	let snapshot = lease
		.publish(policy, collected)
		.expect("cache errors do not block inventories");
	assert_eq!(snapshot.status, InventoryStatus::Ready);
	assert!(snapshot.incomplete);
	assert_eq!(service.state.lock().unwrap().map_members.len(), 1);
	assert!(cache_path.is_dir());
	Ok(())
}
