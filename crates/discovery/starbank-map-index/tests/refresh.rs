use std::fs;

use starbank_map_index::{
	CacheState, Cancellation, IndexLimits, IndexRequest, prepare_map_cache_refresh, scan_maps,
};

#[test]
fn refresh_reuses_warm_records_and_stages_changed_cache_until_publication()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let archive_root = temp.path().join("maps");
	fs::create_dir(&archive_root)?;
	fs::write(
		archive_root.join("alpha.s2ma"),
		include_bytes!("fixtures/alpha.s2ma"),
	)?;
	let cache_path = temp.path().join("metadata.json");
	let request = IndexRequest {
		roots: std::slice::from_ref(&archive_root),
		cache_path: Some(&cache_path),
		generation: 8,
		limits: IndexLimits::default(),
	};
	scan_maps(request, &Cancellation::default(), |_| {})?;
	let previous = fs::read(&cache_path)?;

	let (warm, update) = prepare_map_cache_refresh(request, &Cancellation::default(), |_| {})?;
	assert_eq!(warm.cache_hits, 1);
	assert!(update.is_none());
	fs::write(
		archive_root.join("rival.s2ma"),
		include_bytes!("fixtures/rival.s2ma"),
	)?;

	let (next, update) = prepare_map_cache_refresh(request, &Cancellation::default(), |_| {})?;
	assert_eq!(next.cache_hits, 1);
	assert_eq!(next.index.maps().len(), 2);
	assert_eq!(fs::read(&cache_path)?, previous);
	drop(update.expect("refresh staged the modified cache"));
	assert_eq!(fs::read(&cache_path)?, previous);

	let (_, update) = prepare_map_cache_refresh(request, &Cancellation::default(), |_| {})?;
	update
		.expect("refresh staged the modified cache again")
		.publish()?;
	assert_ne!(fs::read(cache_path)?, previous);
	Ok(())
}

#[test]
fn refresh_reparses_unchanged_archives_from_the_previous_metadata_parser()
-> Result<(), Box<dyn std::error::Error>> {
	const PREVIOUS_CACHE_VERSION: u32 = 1;

	let temp = tempfile::tempdir()?;
	let archive_root = temp.path().join("maps");
	fs::create_dir(&archive_root)?;
	fs::write(
		archive_root.join("alpha.s2ma"),
		include_bytes!("fixtures/alpha.s2ma"),
	)?;
	let cache_path = temp.path().join("metadata.json");
	let request = IndexRequest {
		roots: std::slice::from_ref(&archive_root),
		cache_path: Some(&cache_path),
		generation: 1,
		limits: IndexLimits::default(),
	};
	let scan = || scan_maps(request, &Cancellation::default(), |_| {});
	assert_eq!(scan()?.index.maps().len(), 1);
	assert_eq!(scan()?.cache_hits, 1);

	let mut legacy: serde_json::Value = serde_json::from_slice(&fs::read(&cache_path)?)?;
	legacy["version"] = PREVIOUS_CACHE_VERSION.into();
	let records = legacy["records"].as_object_mut().expect("cache records");
	let record = records.values_mut().next().expect("cached archive");
	record["metadata"]["title"] = "Previous parser title".into();
	fs::write(&cache_path, serde_json::to_vec(&legacy)?)?;

	let refreshed = scan()?;
	assert_eq!(refreshed.cache_state, CacheState::Rebuilding);
	assert_eq!(refreshed.cache_hits, 0);
	assert_eq!(
		refreshed.index.maps()[0].metadata.title.as_deref(),
		Some("Fixture Alpha")
	);
	let warm = scan()?;
	assert_eq!(warm.cache_hits, 1);
	assert_eq!(warm.index.maps(), refreshed.index.maps());
	Ok(())
}
