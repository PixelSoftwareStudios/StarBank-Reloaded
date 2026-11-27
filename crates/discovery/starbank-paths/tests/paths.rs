use std::{
	fs, io,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use starbank_paths::{
	LimitKind, PathError, ScanLimits, SelectionKind, inventory_banks, is_valid_bank_path,
	parse_signing_context,
};

fn write_fixture(root: &Path, path: &str) -> io::Result<PathBuf> {
	let absolute = root.join(path);
	let parent = absolute
		.parent()
		.ok_or_else(|| io::Error::other("test fixture has no parent"))?;
	fs::create_dir_all(parent)?;
	// Deliberately omit bank XML. Discovery must depend on filesystem metadata.
	fs::write(&absolute, b"not XML")?;
	Ok(absolute)
}

#[test]
fn inventory_keeps_all_bank_scopes_and_distinct_name_collisions()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?.join("StarCraft II");
	let nested = write_fixture(
		&root,
		"Accounts/123/1-S2-1-987/Banks/1-S2-1-123/.private/deeper/Shared.Sc2BaNk",
	)?;
	let other_account = write_fixture(
		&root,
		"Accounts/789/2-S2-2-987/Banks/2-S2-2-123/Shared.SC2Bank",
	)?;
	let direct = write_fixture(&root, "Accounts/123/1-S2-1-987/Banks/direct.sc2bank")?;
	let legacy = write_fixture(&root, "Accounts/456/Banks/legacy/nested/historical.SC2Bank")?;
	let backup = write_fixture(
		&root,
		"Accounts/123/1-S2-1-987/Banks/Shared_20000101_000000.sc2bank.BAK",
	)?;
	write_fixture(&root, "Accounts/123/1-S2-1-987/Replays/outside.sc2bank")?;
	write_fixture(&root, "Readme.sc2bank")?;
	fs::create_dir_all(root.join("Accounts/999/3-S2-1-987/Banks"))?;

	let selection = is_valid_bank_path(&root, ScanLimits::default())?;
	assert_eq!(selection.kind(), SelectionKind::GameRoot);
	assert_eq!(selection.bank_roots().len(), 4);
	assert_eq!(
		selection.bank_roots()[0].scope().account.as_deref(),
		Some("123".as_ref())
	);
	assert_eq!(selection.bank_roots()[1].scope().player, None);
	let inventory = inventory_banks(&selection, ScanLimits::default())?;
	assert_eq!(inventory.banks.len(), 4);
	assert_eq!(inventory.backups, [backup]);
	let locations = inventory
		.banks
		.iter()
		.map(|bank| &bank.locations[0])
		.collect::<Vec<_>>();
	let paths = locations
		.iter()
		.map(|location| &location.path)
		.collect::<Vec<_>>();
	for path in [&nested, &other_account, &direct, &legacy] {
		assert!(paths.contains(&path));
	}
	let hint = locations
		.iter()
		.find(|entry| entry.path == nested)
		.and_then(|entry| entry.signing_hint.as_ref())
		.ok_or("nested canonical context missing")?;
	assert_eq!(hint.player, "1-S2-1-987");
	assert_eq!(hint.author, "1-S2-1-123");
	assert_eq!(hint.bank_name, "Shared");
	assert!(
		locations
			.iter()
			.find(|entry| entry.path == direct)
			.is_some_and(|entry| entry.signing_hint.is_none())
	);

	let accounts = is_valid_bank_path(&root.join("Accounts"), ScanLimits::default())?;
	assert_eq!(accounts.kind(), SelectionKind::Accounts);
	assert_eq!(
		inventory_banks(&accounts, ScanLimits::default())?,
		inventory
	);

	let author_path = root.join("Accounts/123/1-S2-1-987/Banks/1-S2-1-123");
	let author = is_valid_bank_path(&author_path, ScanLimits::default())?;
	assert_eq!(author.kind(), SelectionKind::Collection);
	let author_inventory = inventory_banks(&author, ScanLimits::default())?;
	assert_eq!(author_inventory.banks.len(), 1);
	let author_location = &author_inventory.banks[0].locations[0];
	assert_eq!(author_location.path, nested);
	assert_eq!(author_location.scope, *selection.bank_roots()[0].scope());
	Ok(())
}

#[test]
fn manual_collections_preserve_per_bank_scope_through_nested_accounts_and_folders()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let wrapper = temp.path().canonicalize()?.join("Imported collections");
	let first = write_fixture(
		&wrapper,
		"First/Accounts/123/1-S2-1-987/Banks/1-S2-1-123/Nested/Banks/Shared.SC2Bank",
	)?;
	let second = write_fixture(
		&wrapper,
		"Second/Accounts/456/1-S2-1-987/Banks/1-S2-1-456/Shared.SC2Bank",
	)?;
	let loose = write_fixture(&wrapper, "Loose/Shared.SC2Bank")?;
	let ambiguous = write_fixture(
		&wrapper,
		"Third/Accounts/789/1-S2-1-231/Banks/1-S2-1-567/Accounts/456/1-S2-1-987/Banks/1-S2-1-456/Shared.SC2Bank",
	)?;
	let limits = ScanLimits::default();
	let selection = is_valid_bank_path(&wrapper, limits)?;
	assert_eq!(selection.kind(), SelectionKind::Collection);
	let inventory = inventory_banks(&selection, limits)?;
	assert_eq!(inventory.banks.len(), 4);
	let location = |path: &Path| {
		inventory
			.banks
			.iter()
			.flat_map(|bank| &bank.locations)
			.find(|bank| bank.path == path)
	};
	for (path, account, author) in [
		(&first, "123", "1-S2-1-123"),
		(&second, "456", "1-S2-1-456"),
	] {
		let bank = location(path).ok_or("manual bank location")?;
		assert_eq!(bank.scope.account.as_deref(), Some(account.as_ref()));
		assert_eq!(bank.scope.player.as_deref(), Some("1-S2-1-987"));
		let hint = bank.signing_hint.as_ref().ok_or("canonical signing hint")?;
		assert_eq!(hint.author, author);
		assert_eq!(hint.player, "1-S2-1-987");
	}
	let first_location = location(&first).ok_or("first bank")?;
	let scoped_dir = first.parent().ok_or("nested Banks directory")?;
	let author_dir = wrapper.join("First/Accounts/123/1-S2-1-987/Banks/1-S2-1-123");
	for selected in [scoped_dir, author_dir.as_path(), first.as_path()] {
		let selection = is_valid_bank_path(selected, limits)?;
		assert_eq!(selection.bank_roots()[0].scope(), &first_location.scope);
		let single = inventory_banks(&selection, limits)?;
		assert_eq!(single.banks.len(), 1);
		assert_eq!(
			single.banks[0].locations,
			std::slice::from_ref(first_location)
		);
	}
	for path in [loose, ambiguous] {
		let bank = location(&path).ok_or("unknown bank location")?;
		assert!(bank.scope.account.is_none() && bank.scope.player.is_none());
		assert!(bank.signing_hint.is_none());
	}
	Ok(())
}

#[test]
fn accepts_explicit_folders_and_files_without_assuming_signing_context()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let empty = root.join("bAnKs");
	fs::create_dir(&empty)?;
	let selection = is_valid_bank_path(&empty, ScanLimits::default())?;
	assert_eq!(selection.kind(), SelectionKind::Banks);
	assert!(
		inventory_banks(&selection, ScanLimits::default())?
			.banks
			.is_empty()
	);

	let collection = root.join("Hand-picked");
	let file = write_fixture(&collection, "subfolder/One.sc2bank")?;
	let sibling = write_fixture(&collection, "Two.sc2bank")?;
	let inside = write_fixture(&collection, "Banks/Inside.sc2bank")?;
	write_fixture(&collection, "Variables.txt")?;
	write_fixture(&collection, "ExecuteInfo.txt")?;
	let selection = is_valid_bank_path(&collection, ScanLimits::default())?;
	assert_eq!(selection.kind(), SelectionKind::Collection);
	let inventory = inventory_banks(&selection, ScanLimits::default())?;
	let locations = inventory
		.banks
		.iter()
		.map(|bank| &bank.locations[0].path)
		.collect::<Vec<_>>();
	assert_eq!(locations, [&inside, &sibling, &file]);
	let selected_file = is_valid_bank_path(&file, ScanLimits::default())?;
	assert_eq!(selected_file.kind(), SelectionKind::File);
	let direct = inventory_banks(&selected_file, ScanLimits::default())?;
	assert_eq!(direct.banks.len(), 1);
	assert_eq!(direct.banks[0].locations[0].path, file);
	assert_eq!(direct.banks[0].locations[0].signing_hint, None);
	assert!(matches!(
		is_valid_bank_path(Path::new(""), ScanLimits::default()),
		Err(PathError::InvalidSelection(_))
	));
	let invalid = write_fixture(&collection, "not-a-bank.txt")?;
	assert!(matches!(
		is_valid_bank_path(&invalid, ScanLimits::default()),
		Err(PathError::InvalidSelection(_))
	));
	Ok(())
}

#[test]
fn manual_collection_keeps_loose_banks_when_accounts_contributes_no_roots()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let collection = root.join("Selected collection");
	let loose = write_fixture(&collection, "Loose.SC2Bank")?;
	let accounts = collection.join("Accounts");
	fs::create_dir(&accounts)?;
	let limits = ScanLimits::default();

	let selected = is_valid_bank_path(&collection, limits)?;
	assert_eq!(selected.kind(), SelectionKind::Collection);
	let inventory = inventory_banks(&selected, limits)?;
	assert_eq!(inventory.banks.len(), 1);
	let bank = &inventory.banks[0].locations[0];
	assert_eq!(bank.path, loose);
	assert!(bank.signing_hint.is_none());

	let explicit_accounts = is_valid_bank_path(&accounts, limits)?;
	assert_eq!(explicit_accounts.kind(), SelectionKind::Accounts);
	assert!(
		inventory_banks(&explicit_accounts, limits)?
			.banks
			.is_empty()
	);

	let game = root.join("StarCraft II");
	write_fixture(&game, "Outside.SC2Bank")?;
	fs::create_dir(game.join("Accounts"))?;
	let game_root = is_valid_bank_path(&game, limits)?;
	assert_eq!(game_root.kind(), SelectionKind::GameRoot);
	assert!(game_root.bank_roots().is_empty());
	assert!(inventory_banks(&game_root, limits)?.banks.is_empty());

	let scoped = write_fixture(&accounts, "123/1-S2-1-98/Banks/1-S2-1-76/Scoped.SC2Bank")?;
	let recognized = is_valid_bank_path(&collection, limits)?;
	assert_eq!(recognized.kind(), SelectionKind::GameRoot);
	let inventory = inventory_banks(&recognized, limits)?;
	assert_eq!(inventory.banks.len(), 1);
	assert_eq!(inventory.banks[0].locations[0].path, scoped);
	Ok(())
}

#[test]
fn refuses_unbounded_partial_results() -> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().join("Banks");
	write_fixture(&root, "nested/One.sc2bank")?;
	write_fixture(&root, "Two.sc2bank")?;
	let selection = is_valid_bank_path(&root, ScanLimits::default())?;
	let depth_limit = ScanLimits::new(0, NonZeroUsize::new(5).ok_or("nonzero limit")?);
	assert!(matches!(
		inventory_banks(&selection, depth_limit),
		Err(PathError::LimitReached {
			kind: LimitKind::Depth,
			..
		})
	));
	let count_limit = ScanLimits::new(8, NonZeroUsize::MIN);
	assert!(matches!(
		inventory_banks(&selection, count_limit),
		Err(PathError::LimitReached {
			kind: LimitKind::Entries,
			..
		})
	));
	Ok(())
}

#[test]
fn refuses_ambiguous_or_relative_signing_hints() -> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path();
	for path in [
		PathBuf::from("Accounts/123/1-S2-1-99/Banks/1-S2-1-42/Relative.sc2bank"),
		root.join("Accounts/NotNumeric/1-S2-1-99/Banks/1-S2-1-42/Unknown.sc2bank"),
		root.join("Accounts/123/1-S2-1-99/Banks/NameOnly/Unknown.sc2bank"),
		root.join("Accounts/123/1-S2-1-99/Banks/1-S2-1-42/layer/Accounts/123/1-S2-1-99/Banks/1-S2-1-55/Ambiguous.sc2bank"),
		root.join("Accounts/123/1-S2-1-99/Banks/1-S2-1-42/../Ambiguous.sc2bank"),
	] {
		assert_eq!(parse_signing_context(&path), None, "{}", path.display());
	}
	Ok(())
}

#[cfg(unix)]
#[test]
fn ignores_symlinks_and_tracks_hardlink_aliases_without_duplicate_banks()
-> Result<(), Box<dyn std::error::Error>> {
	use std::os::unix::fs::symlink;

	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let directory = root.join("Banks");
	let original = write_fixture(&directory, "Original.sc2bank")?;
	let outside = write_fixture(&root, "Outside/External.sc2bank")?;
	symlink(root.join("Outside"), directory.join("outside-dir"))?;
	symlink(outside, directory.join("outside-file.sc2bank"))?;
	symlink(&directory, directory.join("loop"))?;
	let alias = directory.join("Alias.sc2bank");
	fs::hard_link(&original, &alias)?;

	let selection = is_valid_bank_path(&directory, ScanLimits::default())?;
	let inventory = inventory_banks(&selection, ScanLimits::default())?;
	assert_eq!(inventory.banks.len(), 1);
	assert_eq!(
		inventory.banks[0]
			.locations
			.iter()
			.map(|location| &location.path)
			.collect::<Vec<_>>(),
		[&alias, &original]
	);
	Ok(())
}

#[cfg(unix)]
#[test]
fn permission_failure_is_distinct_from_an_empty_bank_folder()
-> Result<(), Box<dyn std::error::Error>> {
	use std::os::unix::fs::PermissionsExt;

	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?.join("Banks");
	let secret = root.join("unreadable");
	fs::create_dir_all(&secret)?;
	let selection = is_valid_bank_path(&root, ScanLimits::default())?;
	fs::set_permissions(&secret, fs::Permissions::from_mode(0o000))?;
	let inaccessible = fs::read_dir(&secret).is_err();
	let result = inventory_banks(&selection, ScanLimits::default());
	fs::set_permissions(&secret, fs::Permissions::from_mode(0o700))?;
	if inaccessible {
		assert!(
			matches!(result, Err(PathError::Io { path, source }) if path == secret && source.kind() == io::ErrorKind::PermissionDenied)
		);
	}
	Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn linux_auto_detection_is_explicitly_deferred() {
	assert!(matches!(
		starbank_paths::get_sc2_root_path(ScanLimits::default()),
		Err(PathError::NotImplemented)
	));
}
