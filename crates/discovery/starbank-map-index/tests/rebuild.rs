use std::fs;

use starbank_map_index::{
	CacheState, Cancellation, IndexLimits, IndexRequest, ScanError, prepare_map_cache_rebuild,
	scan_maps,
};

#[test]
fn explicit_rebuild_skips_valid_cache_and_requires_publication()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let map_root = temp.path().join("maps");
	fs::create_dir(&map_root)?;
	let alpha = map_root.join("Alpha.s2ma");
	fs::write(&alpha, include_bytes!("fixtures/alpha.s2ma"))?;
	let alpha = alpha.canonicalize()?;
	let cache_path = temp.path().join("metadata.json");
	let request = IndexRequest {
		roots: &[map_root],
		cache_path: Some(&cache_path),
		generation: 8,
		limits: IndexLimits::default(),
	};
	let scan = || scan_maps(request, &Cancellation::default(), |_| {});
	let original = scan()?;
	let original_cache = fs::read(&cache_path)?;
	let mut old_cache: serde_json::Value = serde_json::from_slice(&original_cache)?;
	old_cache["records"][alpha.to_str().expect("fixture path")]["metadata"]["title"] =
		serde_json::json!("Stale but compatible map title");
	let old_bytes = serde_json::to_vec(&old_cache)?;
	fs::write(&cache_path, &old_bytes)?;

	let warm = scan()?;
	assert_eq!(warm.cache_hits, 1);
	assert_ne!(warm.index.maps(), original.index.maps());
	let cancellation = Cancellation::default();
	assert!(matches!(
		prepare_map_cache_rebuild(request, &cancellation, |progress| {
			if progress.completed == progress.total {
				cancellation.cancel();
			}
		}),
		Err(ScanError::Cancelled)
	));
	assert_eq!(fs::read(&cache_path)?, old_bytes);

	let prepare = || prepare_map_cache_rebuild(request, &Cancellation::default(), |_| {});
	let (rebuilt, staged) = prepare()?;
	assert_eq!(
		(rebuilt.cache_state, rebuilt.cache_hits),
		(CacheState::Rebuilding, 0)
	);
	assert_eq!(rebuilt.index.maps(), original.index.maps());
	assert_eq!(fs::read(&cache_path)?, old_bytes);
	drop(staged.expect("completed cache preparation"));
	assert_eq!(fs::read(&cache_path)?, old_bytes);

	let (_, staged) = prepare()?;
	staged.expect("completed cache preparation").publish()?;
	assert_eq!(fs::read(&cache_path)?, original_cache);
	assert_eq!(scan()?.cache_hits, 1);

	fs::write(alpha, b"unreadable archive")?;
	let (partial, staged) = prepare()?;
	assert!(!partial.issues.is_empty());
	assert!(staged.is_none());
	assert_eq!(fs::read(&cache_path)?, original_cache);
	Ok(())
}
