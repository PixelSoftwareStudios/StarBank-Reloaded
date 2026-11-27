#![cfg(windows)]

use std::{fs, os::windows::fs::OpenOptionsExt};

use starbank_map_index::{CacheState, Cancellation, IndexLimits, IndexRequest, scan_maps};

#[test]
fn windows_cache_detects_identity_only_replacement_and_reuses_without_data_access()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().join("maps");
	fs::create_dir(&root)?;
	let archive = root.join("Map.s2ma");
	let retained = root.join("retained");
	let bytes = include_bytes!("fixtures/alpha.s2ma");
	fs::write(&archive, bytes)?;
	let cache = temp.path().join("index.json");
	let request = IndexRequest {
		roots: std::slice::from_ref(&root),
		cache_path: Some(&cache),
		generation: 1,
		limits: IndexLimits::default(),
	};
	let scan = || scan_maps(request, &Cancellation::default(), |_| {});
	let original = scan()?;
	assert!(original.issues.is_empty(), "{:?}", original.issues);
	assert_eq!(original.index.maps().len(), 1);
	assert_eq!(scan()?.cache_hits, 1);

	// Keep the current parser version so this exercises the optional identity
	// field. The cache loads, but its identity-less record must miss.
	let mut legacy: serde_json::Value = serde_json::from_slice(&fs::read(&cache)?)?;
	let records = legacy["records"].as_object_mut().expect("cache records");
	assert_eq!(records.len(), 1);
	let record = records.values_mut().next().expect("cached archive");
	assert!(
		record["fingerprint"]
			.as_object_mut()
			.expect("fingerprint")
			.remove("windows_identity")
			.is_some()
	);
	fs::write(&cache, serde_json::to_vec(&legacy)?)?;
	let migrated = scan()?;
	assert!(migrated.issues.is_empty(), "{:?}", migrated.issues);
	assert_eq!(migrated.cache_state, CacheState::Loaded);
	assert_eq!(migrated.cache_hits, 0);
	assert_eq!(scan()?.cache_hits, 1);

	// Keep the old file live so the file system cannot reuse its identity.
	fs::rename(&archive, &retained)?;
	let original_metadata = fs::metadata(retained)?;
	let mtime = original_metadata.modified()?;
	fs::write(&archive, bytes)?;
	fs::OpenOptions::new()
		.write(true)
		.open(&archive)?
		.set_times(fs::FileTimes::new().set_modified(mtime))?;
	let replacement_metadata = fs::metadata(&archive)?;
	assert_eq!(replacement_metadata.len(), original_metadata.len());
	assert_eq!(replacement_metadata.modified()?, mtime);

	let replacement = scan()?;
	assert!(replacement.issues.is_empty(), "{:?}", replacement.issues);
	assert_eq!(
		replacement.cache_hits, 0,
		"different live file at the same path"
	);
	assert_eq!(replacement.index.maps().len(), 1);

	// Exclusive data access still permits attribute-only identity observation.
	let _deny_new_data_handles = fs::OpenOptions::new()
		.read(true)
		.share_mode(0)
		.open(&archive)?;
	assert!(
		fs::File::open(&archive).is_err(),
		"test must deny a new data handle"
	);
	let warm = scan()?;
	assert!(warm.issues.is_empty(), "{:?}", warm.issues);
	assert_eq!(warm.cache_hits, 1);
	assert_eq!(warm.index.maps(), replacement.index.maps());
	Ok(())
}
