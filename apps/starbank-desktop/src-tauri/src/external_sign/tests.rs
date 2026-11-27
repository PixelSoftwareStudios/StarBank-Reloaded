use std::{fs, io};

use super::*;

const MAIN: &str = "main";
const OTHER_WINDOW: &str = "other";
const BANK: &[u8] = b"<Bank><Section name='Score'/></Bank>";
const CHANGED: &[u8] = b"<Bank><Section name='Changed'/></Bank>";

fn manual(name: &str) -> SigningContext {
	SigningContext::new("1-S2-1-11", "2-S2-1-22", name).expect("synthetic context")
}

fn write_fixture(root: &Path, name: &str) -> io::Result<PathBuf> {
	let path = root.join(name);
	fs::create_dir_all(path.parent().expect("fixture has a parent"))?;
	fs::write(&path, BANK)?;
	Ok(path)
}

#[test]
fn revoked_and_cross_window_sources_are_refused() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let first = write_fixture(&root, "First.SC2Bank")?;
	let second = write_fixture(&root, "Second.SC2Bank")?;
	let mut registry = SourceRegistry::default();
	let first = registry.select(MAIN, first).expect("authorized fixture");
	assert!(first.requires_context);
	assert!(matches!(first.signature_status, CheckedSignature::Unsigned));
	assert!(registry.for_owner(OTHER_WINDOW, &first.id).is_err());
	registry.discard(OTHER_WINDOW, &first.id);
	assert!(registry.for_owner(MAIN, &first.id).is_ok());
	let second = registry.select(MAIN, second).expect("authorized fixture");
	assert!(registry.for_owner(MAIN, &first.id).is_err());
	registry.discard(MAIN, &second.id);
	assert!(registry.for_owner(MAIN, &second.id).is_err());
	Ok(())
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn create_only_export_keeps_source_and_consumes_the_handle() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let source = write_fixture(&root, "Source.SC2Bank")?;
	let existing = write_fixture(&root, "Existing.SC2Bank")?;
	let destination = root.join("Signed copy.SC2Bank");
	let context = manual("Original.Game.Stats");
	let mut registry = SourceRegistry::default();
	let selected = registry
		.select(MAIN, source.clone())
		.expect("authorized fixture");

	for occupied in [&source, &existing] {
		assert!(matches!(
			registry.publish(MAIN, &selected.id, occupied, Some(context.clone())),
			Err(CommandError::WriteRejected(_))
		));
		assert_eq!(fs::read(occupied)?, BANK);
	}
	assert_eq!(
		registry
			.publish(MAIN, &selected.id, &destination, Some(context.clone()))
			.expect("publish verified copy"),
		"Signed copy.SC2Bank"
	);
	let signed = parse(&fs::read(&destination)?, Limits::default()).expect("signed copy XML");
	assert_eq!(
		verify_signature(&signed, Some(&context)),
		starbank_signature::SignatureStatus::Valid
	);
	assert_eq!(fs::read(&source)?, BANK);
	assert!(registry.for_owner(MAIN, &selected.id).is_err());
	Ok(())
}

#[test]
fn an_external_change_invalidates_preview_and_export() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let source = write_fixture(&root, "External.SC2Bank")?;
	let mut registry = SourceRegistry::default();
	let selected = registry
		.select(MAIN, source.clone())
		.expect("authorized fixture");
	fs::write(&source, CHANGED)?;
	assert!(
		registry
			.for_owner(MAIN, &selected.id)
			.expect("issued handle")
			.read()
			.is_err()
	);
	let destination = root.join("Copy.SC2Bank");
	assert!(matches!(
		registry.publish(MAIN, &selected.id, &destination, Some(manual("Stats"))),
		Err(CommandError::ActiveWriteRejected(_))
	));
	assert!(!destination.exists());
	assert_eq!(fs::read(&source)?, CHANGED);
	Ok(())
}

#[test]
fn signing_context_follows_canonical_destination_and_requires_intent_elsewhere() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let path = root.join("StarCraft II/Accounts/123/2-S2-1-22/Banks/1-S2-1-11/Stats.SC2Bank");
	let renamed = path.with_file_name("Renamed.SC2Bank");
	let external = root.join("External/Copy.SC2Bank");
	assert_eq!(
		signed_copy_context(&path, &renamed, None).expect("destination context"),
		manual("Renamed")
	);
	assert_eq!(
		signed_copy_context(&path, &external, None).expect("source context"),
		manual("Stats")
	);
	assert!(signed_copy_context(&external, &external, None).is_err());
	assert_eq!(
		signed_copy_context(&external, &external, Some(manual("Exact.Game.Name")))
			.expect("manual context"),
		manual("Exact.Game.Name")
	);
	assert!(signed_copy_context(&external, &renamed, Some(manual("Wrong"))).is_err());
	assert_eq!(
		signed_copy_context(&external, &renamed, Some(manual("Renamed")))
			.expect("matching canonical context"),
		manual("Renamed")
	);
	Ok(())
}

#[test]
fn uncertain_signed_copy_consumes_the_foreign_source_and_preserves_recovery_evidence()
-> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let source = write_fixture(&root, "External.SC2Bank")?;
	let original = fs::read(&source)?;
	let mut registry = SourceRegistry::default();
	let selected = registry
		.select(MAIN, source.clone())
		.expect("selected source");
	let destination = root.join("Signed Copy.SC2Bank");
	let artifact = root.join(".starbank-pending-signed-copy");
	let error = registry.uncertain_export(
		&destination,
		Some(&artifact),
		&starbank_fs::StorageError::Conflict,
	);
	let encoded = serde_json::to_value(error).expect("native error serializes");
	let receipt = &encoded["recovery_required"];
	assert_eq!(receipt["operation"], "external_signed_copy");
	assert_eq!(receipt["affectsActive"], false);
	assert_eq!(
		receipt["destination"]["display"],
		destination.to_string_lossy().as_ref()
	);
	assert_eq!(receipt["recovery"]["kind"], "recorded");
	assert_eq!(
		receipt["recovery"]["path"]["display"],
		artifact.to_string_lossy().as_ref()
	);
	assert_eq!(receipt["backup"]["kind"], "not_created");
	assert!(registry.for_owner(MAIN, &selected.id).is_err());
	assert!(registry.for_owner(OTHER_WINDOW, &selected.id).is_err());
	assert!(matches!(
		registry.publish(MAIN, &selected.id, &destination, Some(manual("Stats"))),
		Err(CommandError::ActiveWriteRejected(_))
	));
	assert_eq!(fs::read(source)?, original);
	assert!(
		!destination.exists(),
		"failure mapping must not replay the export"
	);
	assert!(
		!artifact.exists(),
		"the receipt records a location without probing or creating it"
	);
	Ok(())
}
