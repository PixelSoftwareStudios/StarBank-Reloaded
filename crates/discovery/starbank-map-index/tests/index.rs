use std::{
	fs,
	path::{Path, PathBuf},
	time::{Duration, UNIX_EPOCH},
};

use starbank_map_index::{
	CacheState, Cancellation, IndexIssueKind, IndexLimit, IndexLimits, IndexRequest,
	MapTitleSource, MetadataError, ScanError, parse_metadata, scan_maps,
};
use starbank_paths::{BankInventory, ScanLimits, inventory_banks, is_valid_bank_path};

const ALPHA: &[u8] = include_bytes!("fixtures/alpha.s2ma");
const RIVAL: &[u8] = include_bytes!("fixtures/rival.s2ma");
const EMPTY: &[u8] = include_bytes!("fixtures/empty.s2ma");

fn bank(root: &Path, relative: &str) -> std::io::Result<PathBuf> {
	let file = root.join(relative);
	fs::create_dir_all(
		file.parent()
			.ok_or_else(|| std::io::Error::other("fixture has no parent"))?,
	)?;
	fs::write(&file, b"not parsed by map indexing")?;
	Ok(file)
}

fn maps(directory: &Path) -> std::io::Result<[PathBuf; 3]> {
	let output = [
		directory.join("Alpha.S2MA"),
		directory.join("Rival.SC2Map"),
		directory.join("Without.SC2Mod"),
	];
	for (index, (path, contents)) in output.iter().zip([ALPHA, RIVAL, EMPTY]).enumerate() {
		fs::write(path, contents)?;
		fs::File::open(path)?.set_modified(UNIX_EPOCH + Duration::from_secs(60 + index as u64))?;
	}
	Ok(output)
}

#[test]
fn archives_keep_collisions_and_unmatched_banks_with_account_scope()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let cache = root.join("Cache");
	fs::create_dir(&cache)?;
	let [alpha, rival, empty] = maps(&cache)?;
	let banks = root.join("StarCraft II");
	let player_a = bank(
		&banks,
		"Accounts/123/1-S2-1-001/Banks/1-S2-1-111/Shared.SC2Bank",
	)?;
	let player_b = bank(
		&banks,
		"Accounts/321/2-S2-2-002/Banks/2-S2-2-222/shared.sc2bank",
	)?;
	let second = bank(
		&banks,
		"Accounts/321/2-S2-2-002/Banks/2-S2-2-222/Second.sc2bank",
	)?;
	let unknown = bank(
		&banks,
		"Accounts/321/2-S2-2-002/Banks/2-S2-2-222/.Unmapped.sc2bank",
	)?;
	let bank_selection = is_valid_bank_path(&banks, ScanLimits::default())?;
	let inventory = inventory_banks(&bank_selection, ScanLimits::default())?;
	let roots = [cache];
	let request = IndexRequest {
		roots: &roots,
		cache_path: None,
		generation: 17,
		limits: IndexLimits::default(),
	};
	let mut progress = Vec::new();
	let report = scan_maps(request, &Cancellation::default(), |update| {
		progress.push((
			update.generation,
			update.completed,
			update.total,
			update.record.is_some(),
		));
	})?;
	assert_eq!(
		progress,
		[(17, 1, 3, true), (17, 2, 3, true), (17, 3, 3, true)]
	);
	assert_eq!(report.cache_state, CacheState::Disabled);
	assert!(report.issues.is_empty());
	assert_eq!(
		report
			.index
			.maps()
			.iter()
			.map(|map| &map.archive)
			.collect::<Vec<_>>(),
		[&empty, &rival, &alpha]
	);

	let matches = report.index.associate(&inventory);
	let by_path = |path: &Path| {
		inventory
			.banks
			.iter()
			.flat_map(|bank| &bank.locations)
			.find(|bank| bank.path == path)
	};
	let bank_a = by_path(&player_a).ok_or("missing account A")?;
	let bank_b = by_path(&player_b).ok_or("missing account B")?;
	assert_ne!(bank_a.scope, bank_b.scope);
	assert_eq!(matches.candidates(bank_a), [1, 2]);
	assert_eq!(matches.candidates(bank_b), [1, 2]);
	assert_eq!(
		matches.candidates(by_path(&second).ok_or("missing second bank")?),
		[2]
	);
	assert_eq!(
		matches
			.unknown_banks()
			.map(|bank| &bank.path)
			.collect::<Vec<_>>(),
		[&unknown]
	);
	assert_eq!(
		matches
			.maps_without_banks()
			.map(|map| &map.archive)
			.collect::<Vec<_>>(),
		[&empty]
	);
	let alpha_map = &report.index.maps()[2];
	assert_eq!(alpha_map.metadata.declared_blizzard(), Some(true));
	assert_eq!(matches.banks_for_map(alpha_map).count(), 3);
	assert_eq!(
		report
			.index
			.associate(&BankInventory::default())
			.maps_without_banks()
			.count(),
		3
	);
	Ok(())
}

#[test]
fn persisted_cache_reuses_unchanged_archives_and_rebuilds_on_changes()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path();
	let map_root = root.join("maps");
	fs::create_dir(&map_root)?;
	let [alpha, _, empty] = maps(&map_root)?;
	let cache_path = root.join("metadata.json");
	let roots = [map_root];
	let request = IndexRequest {
		roots: &roots,
		cache_path: Some(&cache_path),
		generation: 3,
		limits: IndexLimits::default(),
	};
	let scan = || scan_maps(request, &Cancellation::default(), |_| {});
	let first = scan()?;
	assert_eq!(
		(first.cache_state, first.cache_hits),
		(CacheState::Missing, 0)
	);
	assert!(first.issues.is_empty());
	let unchanged = scan()?;
	assert_eq!(
		(unchanged.cache_state, unchanged.cache_hits),
		(CacheState::Loaded, 3)
	);
	assert_eq!(unchanged.index.maps(), first.index.maps());

	fs::write(&alpha, RIVAL)?;
	fs::remove_file(empty)?;
	let updated = scan()?;
	assert_eq!(updated.cache_hits, 1);
	assert_eq!(updated.index.maps().len(), 2);
	assert!(
		updated
			.index
			.maps()
			.iter()
			.all(|map| map.metadata.title.as_deref() == Some("A Different Map"))
	);
	assert_eq!(scan()?.cache_hits, 2);

	fs::write(&cache_path, b"invalid metadata cache")?;
	let rebuilt = scan()?;
	assert_eq!(
		(rebuilt.cache_state, rebuilt.cache_hits),
		(CacheState::Rebuilding, 0)
	);
	assert_eq!(rebuilt.index.maps(), updated.index.maps());
	assert_eq!(
		(scan()?.cache_state, scan()?.cache_hits),
		(CacheState::Loaded, 2)
	);
	Ok(())
}

#[test]
fn cancellation_limits_and_oversized_archives_are_visible() -> Result<(), Box<dyn std::error::Error>>
{
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let map_root = root.join("maps");
	fs::create_dir(&map_root)?;
	let [alpha, _, _] = maps(&map_root)?;
	let cache_path = root.join("metadata.json");
	let roots = [map_root];
	let mut request = IndexRequest {
		roots: &roots,
		cache_path: Some(&cache_path),
		generation: 7,
		limits: IndexLimits::default(),
	};
	scan_maps(request, &Cancellation::default(), |_| {})?;
	let initial_cache = fs::read(&cache_path)?;
	fs::write(alpha, RIVAL)?;

	let cancellation = Cancellation::default();
	assert!(matches!(
		scan_maps(request, &cancellation, |_| cancellation.cancel()),
		Err(ScanError::Cancelled)
	));
	assert_eq!(fs::read(&cache_path)?, initial_cache);

	request.limits.max_archives = 1;
	assert!(matches!(
		scan_maps(request, &Cancellation::default(), |_| {}),
		Err(ScanError::Limit(IndexLimit::Archives))
	));
	assert_eq!(fs::read(&cache_path)?, initial_cache);

	request.limits.max_archives = 3;
	request.limits.max_metadata_bytes = 1;
	assert!(matches!(
		scan_maps(request, &Cancellation::default(), |_| {}),
		Err(ScanError::Limit(IndexLimit::Metadata))
	));
	assert_eq!(fs::read(&cache_path)?, initial_cache);

	request.limits.max_metadata_bytes = IndexLimits::default().max_metadata_bytes;
	request.limits.max_archive_bytes = 1;
	request.cache_path = None;
	let report = scan_maps(request, &Cancellation::default(), |_| {})?;
	assert!(report.index.maps().is_empty());
	assert_eq!(report.issues.len(), 3);
	assert!(
		report
			.issues
			.iter()
			.all(|issue| matches!(issue.kind, IndexIssueKind::ArchiveTooLarge))
	);
	Ok(())
}

#[cfg(unix)]
#[test]
fn walker_does_not_follow_unselected_archive_or_directory_links()
-> Result<(), Box<dyn std::error::Error>> {
	use std::os::unix::fs::symlink;

	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let cache = root.join("Cache");
	let outside = root.join("Outside");
	fs::create_dir(&cache)?;
	fs::create_dir(&outside)?;
	fs::write(cache.join("Included.s2ma"), ALPHA)?;
	fs::write(outside.join("Secret.s2ma"), RIVAL)?;
	symlink(
		outside.join("Secret.s2ma"),
		cache.join("Archive Alias.s2ma"),
	)?;
	symlink(&outside, cache.join("Directory Alias"))?;
	symlink(&cache, cache.join("Loop"))?;
	let roots = [cache];
	let request = IndexRequest {
		roots: &roots,
		cache_path: None,
		generation: 0,
		limits: IndexLimits::default(),
	};
	let report = scan_maps(request, &Cancellation::default(), |_| {})?;
	assert!(report.issues.is_empty());
	assert_eq!(report.index.maps().len(), 1);
	assert_eq!(
		report.index.maps()[0].metadata.title.as_deref(),
		Some("Fixture Alpha")
	);
	Ok(())
}

#[test]
fn lexer_extracts_static_names_and_leaves_dynamic_assignments_unknown() -> Result<(), MetadataError>
{
	let script = br#"
        // Name: Example Map
        // Author: Star Editor
        string stable = "Fixed";
        string dynamic = "Maybe";
        string ambiguous = "Another";
        string disguised = "BankLoad(\"NotCode\")";
        stable = "FIXED";
        dynamic += GetName();
        ambiguous = "Disagree";
        if (stable == "Ignored") {}
        BankLoad ("DIRECT", 1);
        BankLoad(stable, 1);
        BankLoad(dynamic, 1);
        BankLoad(ambiguous, 1);
        BankLoad("prefix-" + GetName(), 1);
        BankLoad("../Outside", 1);
        // BankLoad("Comment", 1);
        /* BankLoad("Other comment", 1); */
    "#;
	let metadata = parse_metadata(Some(script), None)?;
	assert_eq!(metadata.title.as_deref(), Some("Example Map"));
	assert_eq!(metadata.title_source, Some(MapTitleSource::Script));
	assert_eq!(metadata.author.as_deref(), Some("Star Editor"));
	assert_eq!(metadata.bank_names, ["direct", "fixed"]);
	assert_eq!(metadata.declared_blizzard(), Some(false));
	Ok(())
}

#[test]
fn headers_only_fill_missing_or_placeholder_title() -> Result<(), MetadataError> {
	let complete = b"DocInfo/Name..xyz\0R\xc3\xa9sum\xc3\xa9\x01\0";
	let metadata = parse_metadata(Some(b"// Name: ?????\n"), Some(complete))?;
	assert_eq!(metadata.title.as_deref(), Some("R\u{00e9}sum\u{00e9}"));
	assert_eq!(metadata.title_source, Some(MapTitleSource::DocumentHeader));
	let trusted = parse_metadata(Some(b"// Name: Original title"), Some(complete))?;
	assert_eq!(trusted.title.as_deref(), Some("Original title"));
	assert_eq!(trusted.title_source, Some(MapTitleSource::Script));
	let ambiguous = [
		complete.as_slice(),
		b"DocInfo/Name.....\0Another locale\x01\0",
	]
	.concat();
	assert!(parse_metadata(None, Some(&ambiguous))?.title.is_none());
	assert!(
		parse_metadata(None, Some(b"DocInfo/Name.....\0Truncated"))?
			.title
			.is_none()
	);
	assert_eq!(
		parse_metadata(Some(b"bad utf8 \xff"), None),
		Err(MetadataError::ScriptEncoding)
	);
	Ok(())
}

#[test]
fn large_scripts_resolve_repeated_variable_references_once() -> Result<(), MetadataError> {
	let mut script = String::from("string repeated = \"One\";\n");
	for _ in 0..1_000 {
		script.push_str("BankLoad(repeated, 1);\n/*");
		script.extend(std::iter::repeat_n('a', 9_000));
		script.push_str("*/\n");
	}
	assert!(script.len() > 9_000_000);
	let metadata = parse_metadata(Some(script.as_bytes()), None)?;
	assert_eq!(metadata.bank_names, ["one"]);
	Ok(())
}

#[test]
fn cache_destination_cannot_overwrite_game_archives_or_live_cache_folders()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path();
	let cache_input = root.join("maps");
	fs::create_dir(&cache_input)?;
	let archive = cache_input.join("Original.s2ma");
	fs::write(&archive, ALPHA)?;
	let unrelated = root.join("Unrelated.s2ma");
	fs::write(&unrelated, RIVAL)?;
	let nested_metadata = cache_input.join("metadata.json");
	let roots = [cache_input];
	for destination in [&archive, &unrelated, &nested_metadata] {
		let request = IndexRequest {
			roots: &roots,
			cache_path: Some(destination),
			generation: 0,
			limits: IndexLimits::default(),
		};
		let report = scan_maps(request, &Cancellation::default(), |_| {})?;
		assert_eq!(report.index.maps().len(), 1);
		assert_eq!(report.cache_state, CacheState::Unavailable);
		assert!(
			report
				.issues
				.iter()
				.any(|issue| matches!(issue.kind, IndexIssueKind::CacheRead(_)))
		);
	}
	assert_eq!(fs::read(archive)?, ALPHA);
	assert_eq!(fs::read(unrelated)?, RIVAL);
	assert!(!nested_metadata.exists());
	Ok(())
}
