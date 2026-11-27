use std::{fs, io, path::Path};

use super::*;

const MAP_ARCHIVE: &[u8] =
	include_bytes!("../../../../../crates/discovery/starbank-map-index/tests/fixtures/alpha.s2ma");

fn write(root: &Path, relative: &str, bytes: &[u8]) -> io::Result<()> {
	let path = root.join(relative);
	fs::create_dir_all(path.parent().expect("fixture parent"))?;
	fs::write(path, bytes)
}

#[test]
fn frozen_map_uses_published_bank_links_and_never_accepts_paths() -> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let banks = root.join("banks");
	let maps = root.join("maps");
	write(
		&banks,
		"Accounts/123/1-S2-1-10/Banks/1-S2-1-44/Shared.SC2Bank",
		b"<Bank/>",
	)?;
	write(
		&banks,
		"Accounts/123/1-S2-1-10/Banks/1-S2-1-44/Loose.SC2Bank",
		b"<Bank/>",
	)?;
	write(&maps, "alpha.s2ma", MAP_ARCHIVE)?;
	let service = InventoryService::default();
	let visible = service
		.scan_with_roots(crate::inventory::ScanMode::Manual(banks.clone()), &[maps])
		.expect("published scan");
	let visible = serde_json::to_value(visible).expect("public inventory JSON");
	let map_id = visible["maps"][0]["id"].as_str().expect("opaque map ID");
	let all = service.capture_batch(None).expect("all inventory");
	let map = service.capture_batch(Some(map_id)).expect("published map");
	let expected = visible["banks"]
		.as_array()
		.expect("bank DTOs")
		.iter()
		.filter(|bank| {
			bank["mapIds"]
				.as_array()
				.expect("map ids")
				.iter()
				.any(|id| id == map_id)
		})
		.map(|bank| bank["id"].as_str().expect("bank ID"))
		.collect::<Vec<_>>();
	assert_eq!(all.members.len(), 2);
	assert_eq!(expected.len(), 1);
	assert_eq!(
		map.members
			.iter()
			.map(|member| member.id.as_str())
			.collect::<Vec<_>>(),
		expected
	);
	assert!(matches!(
		service.capture_batch(banks.to_str()),
		Err(InventoryError::UnknownMap)
	));
	let next_scan = service
		.start_scan(crate::inventory::ScanMode::Refresh)
		.expect("refresh");
	assert!(matches!(
		service.capture_batch(None),
		Err(InventoryError::Busy)
	));
	assert!(matches!(
		service.validate_batch_generation(all.generation),
		Err(InventoryError::StaleBank)
	));
	drop(next_scan);
	assert!(service.validate_batch_generation(all.generation).is_err());
	Ok(())
}

#[test]
fn frozen_leaf_detects_a_different_file_at_the_same_path() -> io::Result<()> {
	let fixture = tempfile::tempdir()?;
	let root = fixture.path().canonicalize()?;
	let bank = root.join("Fixture.SC2Bank");
	fs::write(&bank, b"old data")?;
	let plan = scan_batch_fixture(&root).expect("frozen member");
	let member = &plan.members[0];
	member.authority.validate().expect("same initial bank");
	fs::write(&bank, b"edits to same bank")?;
	member
		.authority
		.validate()
		.expect("same file identity after in-place edit");
	let original = root.join("Original.bin");
	fs::rename(&bank, &original)?;
	fs::write(&bank, b"a different file")?;
	#[cfg(windows)]
	{
		use std::os::windows::fs::FileTimesExt as _;

		// Windows can preserve a creation time when the same name is reused.
		// Force equal times so this test also runs with tunneling disabled.
		let created = fs::metadata(&original)?.created()?;
		fs::OpenOptions::new()
			.write(true)
			.open(&bank)?
			.set_times(fs::FileTimes::new().set_created(created))?;
		assert_eq!(fs::metadata(&bank)?.created()?, created);
	}
	assert!(matches!(
		member.authority.validate(),
		Err(InventoryError::StaleBank)
	));
	Ok(())
}
