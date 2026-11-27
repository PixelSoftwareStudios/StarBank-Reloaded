use std::fs;

use starbank_map_index::{
	Cancellation, IndexLimits, IndexRequest, prepare_map_cache_refresh, scan_maps,
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
