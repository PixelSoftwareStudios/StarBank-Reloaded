use std::{error::Error, fs, path::PathBuf};

use super::{
	SETTINGS_FILE_NAME, SettingsIssue, SettingsStore,
	schema::{AppSettings, AutomaticBackup, SettingChange},
};

const MAX_TEST_SETTINGS_BYTES: usize = 64 * 1024;

struct Sandbox {
	_temp: tempfile::TempDir,
	app: PathBuf,
	legacy: PathBuf,
}

impl Sandbox {
	fn new() -> Result<Self, Box<dyn Error>> {
		let temp = tempfile::tempdir()?;
		let root = temp.path().canonicalize()?;
		Ok(Self {
			_temp: temp,
			app: root.join("desktop-config"),
			legacy: root.join("go-config").join(SETTINGS_FILE_NAME),
		})
	}

	fn store(&self) -> SettingsStore {
		SettingsStore::new(self.app.clone(), self.legacy.clone())
	}

	fn current(&self) -> PathBuf {
		self.app.join(SETTINGS_FILE_NAME)
	}

	fn write_current(&self, bytes: &[u8]) -> std::io::Result<()> {
		fs::create_dir_all(&self.app)?;
		fs::write(self.current(), bytes)
	}

	fn write_legacy(&self, bytes: &[u8]) -> std::io::Result<()> {
		fs::create_dir_all(self.legacy.parent().expect("fixture parent"))?;
		fs::write(&self.legacy, bytes)
	}
}

#[test]
fn old_v1_settings_preserve_existing_fields_and_each_worker_limit_persists()
-> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	let mut old = serde_json::to_value(AppSettings::default())?;
	old["theme"] = "dark".into();
	old.as_object_mut()
		.expect("settings object")
		.remove("mapIndexWorkers");
	let original = serde_json::to_vec(&old)?;
	sandbox.write_current(&original)?;

	assert_eq!(sandbox.store().map_index_worker_limit()?, None);
	assert_eq!(fs::read(sandbox.current())?, original);
	for (choice, expected) in [
		("1", Some(1)),
		("2", Some(2)),
		("3", Some(3)),
		("4", Some(4)),
		("auto", None),
	] {
		let change = serde_json::from_value(serde_json::json!({
			"key": "mapIndexWorkers",
			"value": choice
		}))?;
		sandbox.store().change(change)?;
		let reopened = sandbox.store();
		assert_eq!(
			reopened
				.map_index_worker_limit()?
				.map(std::num::NonZeroUsize::get),
			expected,
		);
		let snapshot = serde_json::to_value(reopened.load()?)?;
		assert_eq!(snapshot["settings"]["mapIndexWorkers"], choice);
		assert_eq!(snapshot["settings"]["theme"], "dark");
	}

	for value in [
		serde_json::json!("5"),
		serde_json::json!(1),
		serde_json::Value::Null,
	] {
		assert!(
			serde_json::from_value::<SettingChange>(serde_json::json!({
				"key": "mapIndexWorkers",
				"value": value
			}))
			.is_err()
		);
		old["mapIndexWorkers"] = value;
		assert_eq!(
			AppSettings::decode(&serde_json::to_vec(&old)?),
			Err(SettingsIssue::InvalidSettings)
		);
	}
	Ok(())
}

#[test]
fn separate_backup_preferences_survive_regular_preference_edits() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	let store = sandbox.store();
	let name = "backup-preferences.json";
	let record = br#"{"version":1,"folders":[],"selected":null,"keepNewest":4}"#;
	assert!(store.read_record(name)?.is_none());
	store.update_record(name, |current| {
		assert!(current.is_none());
		Ok((record.to_vec(), ()))
	})?;
	assert!(!sandbox.current().exists());
	store.change(SettingChange::HideBlizzardMaps(false))?;

	let reopened = sandbox.store();
	let rejected = reopened.update_record(name, |current| -> Result<(_, ()), SettingsIssue> {
		assert_eq!(current, Some(record.as_slice()));
		Err(SettingsIssue::InvalidSettings)
	});
	assert_eq!(rejected, Err(SettingsIssue::InvalidSettings));
	assert_eq!(
		reopened
			.read_record(name)?
			.expect("persisted record")
			.bytes(),
		record
	);
	assert_eq!(
		serde_json::to_value(reopened.load()?.settings)?["hideBlizzardMaps"],
		false
	);
	Ok(())
}

#[test]
fn first_load_returns_defaults_without_creating_a_file() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	let first = sandbox.store().load_or_migrate()?;
	assert_eq!(first.settings, AppSettings::default());
	assert!(!first.legacy_root_pending);
	assert_eq!(
		serde_json::to_value(&first)?["automaticBackupAvailable"],
		cfg!(target_os = "macos"),
	);
	assert!(!sandbox.app.exists());
	Ok(())
}

#[test]
fn legacy_preferences_import_when_v1_is_absent_without_exposing_the_root()
-> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	let original = br#"{
		"hide_blizzard_maps": false,
		"hide_maps_without_banks": null,
		"auto_resign_on_save": false,
		"sc2_root_override": "/private/sc2-fixture"
	}"#;
	sandbox.write_legacy(original)?;
	let store = sandbox.store();
	let snapshot = store.load()?;
	let json = serde_json::to_value(&snapshot)?;
	assert_eq!(json["settings"]["hideBlizzardMaps"], false);
	assert_eq!(json["settings"]["hideMapsWithoutBanks"], true);
	assert_eq!(json["settings"]["autoResignOnSave"], false);
	assert_eq!(json["legacyRootPending"], true);
	assert!(!serde_json::to_string(&snapshot)?.contains("sc2-fixture"));
	assert_eq!(
		store.legacy_root_override()?,
		Some(PathBuf::from("/private/sc2-fixture")),
	);
	assert!(!sandbox.current().exists());

	let changed = store.change(SettingChange::WrapRaw(true))?;
	assert_eq!(sandbox.store().load()?, changed);
	assert_eq!(fs::read(&sandbox.legacy)?, original);
	assert!(!fs::read_to_string(sandbox.current())?.contains("sc2-fixture"));

	let preferences = store.save_preferences()?;
	assert!(!preferences.auto_resign_on_save);
	assert_eq!(
		preferences.automatic_backup,
		AutomaticBackup::FirstOverwrite
	);
	Ok(())
}

#[test]
fn startup_publishes_migrated_preferences_once() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	sandbox.write_legacy(br#"{"hide_blizzard_maps":false}"#)?;
	let migrated = sandbox.store().load_or_migrate()?;
	assert_eq!(
		serde_json::to_value(&migrated)?["settings"]["hideBlizzardMaps"],
		false
	);
	assert!(sandbox.current().is_file());
	sandbox.write_legacy(br#"{"hide_blizzard_maps":true}"#)?;
	assert_eq!(sandbox.store().load_or_migrate()?, migrated);
	Ok(())
}

#[test]
fn independent_updates_read_back_through_new_store_instances() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	sandbox
		.store()
		.change(SettingChange::AutoResignOnSave(false))?;
	sandbox.store().change(SettingChange::AutomaticBackup(
		AutomaticBackup::EveryOverwrite,
	))?;
	let preferences = sandbox.store().save_preferences()?;
	assert!(!preferences.auto_resign_on_save);
	assert_eq!(
		preferences.automatic_backup,
		AutomaticBackup::EveryOverwrite
	);
	assert_eq!(
		serde_json::to_value(sandbox.store().load()?)?["settings"]["automaticBackup"],
		"every-overwrite",
	);
	assert!(!fs::read_to_string(sandbox.current())?.contains("automaticBackupAvailable"));
	assert_eq!(
		sandbox.store().recover(),
		Err(SettingsIssue::SettingsChanged)
	);
	Ok(())
}

#[test]
fn recovery_is_explicit_and_preserves_an_ignored_legacy_config() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	sandbox.write_legacy(br#"{"hide_blizzard_maps":false}"#)?;
	sandbox.write_current(b"corrupt preferences")?;
	let store = sandbox.store();
	assert_eq!(store.load(), Err(SettingsIssue::InvalidJson));
	assert_eq!(
		store.change(SettingChange::ShowBreadcrumbs(false)),
		Err(SettingsIssue::InvalidJson),
	);
	assert_eq!(fs::read(sandbox.current())?, b"corrupt preferences");
	assert_eq!(store.recover()?.settings, AppSettings::default());
	assert_eq!(sandbox.store().load()?.settings, AppSettings::default());
	assert_eq!(
		fs::read(&sandbox.legacy)?,
		br#"{"hide_blizzard_maps":false}"#
	);
	Ok(())
}

#[test]
fn future_or_oversized_settings_survive_updates_and_recovery() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	for (original, issue) in [
		(
			br#"{"version": 2, "unknown": "keep"}"#.to_vec(),
			SettingsIssue::UnsupportedVersion,
		),
		(
			vec![b' '; MAX_TEST_SETTINGS_BYTES + 1],
			SettingsIssue::SettingsTooLarge,
		),
	] {
		sandbox.write_current(&original)?;
		let store = sandbox.store();
		assert_eq!(store.load(), Err(issue));
		assert_eq!(store.change(SettingChange::WrapRaw(true)), Err(issue));
		assert_eq!(store.recover(), Err(issue));
		assert_eq!(fs::read(sandbox.current())?, original);
	}
	Ok(())
}

#[test]
fn invalid_and_out_of_bounds_edits_do_not_publish() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	let store = sandbox.store();
	let saved = store.change(SettingChange::MapPaneWidth(40.5))?;
	let bytes = fs::read(sandbox.current())?;
	for invalid in [19.0, 51.0, f64::NAN] {
		assert_eq!(
			store.change(SettingChange::MapPaneWidth(invalid)),
			Err(SettingsIssue::InvalidSettings),
		);
	}
	assert_eq!(store.load()?, saved);
	assert_eq!(fs::read(sandbox.current())?, bytes);
	let change: SettingChange =
		serde_json::from_str(r#"{"key":"hideBlizzardMaps","value":false}"#)?;
	assert_eq!(change, SettingChange::HideBlizzardMaps(false));
	for invalid in [
		r#"{"key":"version","value":2}"#,
		r#"{"key":"theme","value":"automatic"}"#,
		r#"{"key":"wrapRaw","value":"true"}"#,
		r#"{"key":"wrapRaw","value":true,"extra":false}"#,
	] {
		assert!(
			serde_json::from_str::<SettingChange>(invalid).is_err(),
			"{invalid}"
		);
	}
	Ok(())
}

#[test]
fn contention_preserves_current_settings() -> Result<(), Box<dyn Error>> {
	let sandbox = Sandbox::new()?;
	let store = sandbox.store();
	let initial = store.change(SettingChange::WrapRaw(false))?;
	let lock = fs::OpenOptions::new()
		.read(true)
		.write(true)
		.open(sandbox.app.join(".settings.lock"))?;
	lock.try_lock()?;
	assert_eq!(
		store.change(SettingChange::WrapRaw(true)),
		Err(SettingsIssue::SettingsChanged),
	);
	assert_eq!(store.load()?, initial);
	Ok(())
}

#[cfg(unix)]
#[test]
fn settings_replacement_preserves_file_permissions() -> Result<(), Box<dyn Error>> {
	use std::os::unix::fs::PermissionsExt as _;

	let sandbox = Sandbox::new()?;
	let store = sandbox.store();
	store.change(SettingChange::WrapRaw(true))?;
	let current = sandbox.current();
	let mode = 0o640;
	fs::set_permissions(&current, fs::Permissions::from_mode(mode))?;
	store.change(SettingChange::WrapRaw(false))?;
	assert_eq!(fs::metadata(&current)?.permissions().mode() & 0o777, mode);
	Ok(())
}
