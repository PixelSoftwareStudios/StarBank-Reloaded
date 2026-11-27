use super::*;

#[test]
fn worker_preferences_never_exceed_half_the_machine_or_four_workers() {
	for (cpu_count, automatic) in [
		(0, 1),
		(1, 1),
		(2, 1),
		(3, 1),
		(4, 2),
		(7, 3),
		(8, 4),
		(256, 4),
	] {
		let available = NonZeroUsize::new(cpu_count);
		assert_eq!(worker_limit(available, None), automatic);
		for maximum in 1..=6 {
			assert_eq!(
				worker_limit(available, NonZeroUsize::new(maximum)),
				automatic.min(maximum)
			);
		}
	}
}

#[test]
fn warm_worker_checks_that_its_enumerated_archive_is_still_present()
-> Result<(), Box<dyn std::error::Error>> {
	use std::fs;

	let temp = tempfile::tempdir()?;
	let root = temp.path().join("maps");
	fs::create_dir(&root)?;
	let archive = root.join("Map.s2ma");
	let retained = root.join("Retained archive");
	fs::write(&archive, include_bytes!("../../tests/fixtures/alpha.s2ma"))?;
	let cache_path = temp.path().join("metadata.json");
	let cancel = Cancellation::default();
	let limits = IndexLimits::default();
	let request = IndexRequest {
		roots: std::slice::from_ref(&root),
		cache_path: Some(&cache_path),
		generation: 1,
		limits,
	};
	let original = scan_maps(request, &cancel, |_| {})?;
	assert!(original.issues.is_empty());
	let (cache, cache_state, error) = Cache::load(&cache_path);
	assert_eq!(cache_state, CacheState::Loaded);
	assert!(error.is_none());

	let pending = enumerate(request.roots, limits, &cancel)?;
	let [work] = pending.work.as_slice() else {
		panic!("one synthetic archive");
	};
	let (cached, hit) = process_one(work, &cache).expect("unchanged archive cache hit");
	assert!(hit);
	assert_eq!(cached.metadata, original.index.maps()[0].metadata);

	fs::rename(&archive, &retained)?;
	assert!(matches!(
		process_one(work, &cache),
		Err(IndexIssueKind::Io(error)) if error.kind() == io::ErrorKind::NotFound
	));

	fs::create_dir(&archive)?;
	assert!(matches!(
		process_one(work, &cache),
		Err(IndexIssueKind::ArchiveChanged)
	));
	fs::remove_dir(&archive)?;

	#[cfg(unix)]
	{
		std::os::unix::fs::symlink(
			retained.file_name().expect("retained archive name"),
			&archive,
		)?;
		assert!(matches!(
			process_one(work, &cache),
			Err(IndexIssueKind::ArchiveChanged)
		));
		fs::remove_file(&archive)?;
	}

	fs::write(&archive, include_bytes!("../../tests/fixtures/rival.s2ma"))?;
	assert!(matches!(
		process_one(work, &cache),
		Err(IndexIssueKind::ArchiveChanged)
	));
	Ok(())
}
