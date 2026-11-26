use super::*;

#[test]
fn older_installations_start_with_the_app_location_and_keep_all() {
	let preferences = BackupPreferences::decode(None).expect("default preferences");
	assert_eq!(preferences.selected(), None);
	assert_eq!(preferences.keep_newest, None);
	assert_eq!(
		serde_json::to_value(preferences.view()).expect("renderer view"),
		serde_json::json!({"location":{"mode":"default"},"keepNewest":null})
	);
}

#[test]
fn malformed_or_future_preferences_do_not_authorize_default_writes() {
	for (bytes, expected) in [
		(
			br#"{"version":2,"folders":[],"selected":null,"keepNewest":null}"#.as_slice(),
			SettingsIssue::UnsupportedVersion,
		),
		(
			br#"{"version":1,"folders":[],"selected":0,"keepNewest":null}"#.as_slice(),
			SettingsIssue::InvalidSettings,
		),
		(
			br#"{"version":1,"folders":[],"selected":null,"keepNewest":1001}"#.as_slice(),
			SettingsIssue::InvalidSettings,
		),
		(
			br#"{"version":1,"folders":[],"selected":null,"keepNewest":0}"#.as_slice(),
			SettingsIssue::InvalidSettings,
		),
	] {
		assert_eq!(
			BackupPreferences::decode(Some(bytes)).unwrap_err(),
			expected
		);
	}
	assert!(validated_retention(Some(0)).is_err());
	assert!(validated_retention(Some(MAX_KEEP_NEWEST + 1)).is_err());
	assert_eq!(validated_retention(None).unwrap(), None);
	assert_eq!(
		validated_retention(Some(MAX_KEEP_NEWEST)).unwrap(),
		NonZeroUsize::new(MAX_KEEP_NEWEST)
	);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn selected_and_previous_folders_survive_switches_without_publishing_paths()
-> Result<(), Box<dyn std::error::Error>> {
	let scratch = tempfile::tempdir()?;
	let root = scratch.path().canonicalize()?;
	let mut preferences = BackupPreferences::default();
	for name in ["First", "Second"] {
		let path = root.join(name);
		fs::create_dir(&path)?;
		preferences.select(BackupDirectory::capture(&path)?)?;
	}

	let view = serde_json::to_string(&preferences.view())?;
	assert!(view.contains("Second"));
	assert!(!view.contains(root.to_str().expect("test root displayed losslessly")));
	let second = root.join("Second");
	fs::rename(&second, root.join("displaced"))?;
	fs::create_dir(&second)?;
	assert!(
		preferences
			.selected()
			.expect("selected path")
			.validate()
			.is_err()
	);
	assert!(serde_json::to_string(&preferences.view())?.contains("needs_authorization"));

	preferences.selected = None;
	let restored = BackupPreferences::decode(Some(&serde_json::to_vec(&preferences)?))?;
	assert!(restored.selected().is_none());
	assert_eq!(restored.folders.len(), 2);
	assert_eq!(restored.folders[0].path(), root.join("First"));
	assert!(restored.folders[0].validate().is_ok());
	assert!(restored.folders[1].validate().is_err());
	Ok(())
}
