//! Reads and updates desktop preferences within the app's configuration directory.

mod schema;
mod store;

#[cfg(test)]
mod tests;

use std::{fmt, num::NonZeroUsize, path::PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager as _};

use self::schema::SettingChange;

pub(crate) use self::{schema::AutomaticBackup, store::SettingsStore};

const LEGACY_DIRECTORY: &str = "StarBank-Reloaded";
const SETTINGS_FILE_NAME: &str = "settings.json";

/// A renderer-safe result that omits configuration paths and migrated folder names.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsSnapshot {
	settings: schema::AppSettings,
	legacy_root_pending: bool,
	automatic_backup_available: bool,
}

impl SettingsSnapshot {
	fn new(settings: schema::AppSettings, legacy_root_pending: bool) -> Self {
		Self {
			settings,
			legacy_root_pending,
			// Individual overwrite targets still require verified native storage.
			automatic_backup_available: cfg!(target_os = "macos"),
		}
	}
}

/// Desktop save policy read from the same preferences as the Settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SavePreferences {
	pub(crate) auto_resign_on_save: bool,
	pub(crate) automatic_backup: AutomaticBackup,
}

/// A typed failure from loading, validating, or publishing desktop preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SettingsIssue {
	InvalidJson,
	InvalidSettings,
	UnsupportedVersion,
	SettingsTooLarge,
	StorageUnavailable,
	SettingsChanged,
}

impl fmt::Display for SettingsIssue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(match self {
			Self::InvalidJson => "settings JSON could not be read",
			Self::InvalidSettings => "settings contain invalid preferences",
			Self::UnsupportedVersion => "settings version is unsupported",
			Self::SettingsTooLarge => "settings exceed the supported size",
			Self::StorageUnavailable => "settings storage is unavailable",
			Self::SettingsChanged => "settings changed during the operation",
		})
	}
}

impl std::error::Error for SettingsIssue {}

impl SettingsStore {
	/// Binds storage to the app's config directory and the original Go config source.
	///
	/// # Errors
	///
	/// Returns a storage issue when the OS configuration path is unavailable.
	pub(crate) fn for_app(app: &AppHandle) -> Result<Self, SettingsIssue> {
		let paths = app.path();
		let directory = paths
			.app_config_dir()
			.map_err(|_| SettingsIssue::StorageUnavailable)?;
		let legacy = paths
			.config_dir()
			.map_err(|_| SettingsIssue::StorageUnavailable)?
			.join(LEGACY_DIRECTORY)
			.join(SETTINGS_FILE_NAME);
		Ok(Self::new(directory, legacy))
	}

	/// Reads the two preferences that affect native Save.
	///
	/// # Errors
	/// Invalid or unavailable settings cannot supply an effective save policy.
	pub(crate) fn save_preferences(&self) -> Result<SavePreferences, SettingsIssue> {
		Ok(self.load()?.settings.save_preferences())
	}

	/// Reads the current map indexing worker preference.
	///
	/// # Errors
	/// Invalid or unavailable settings cannot supply a saved worker limit.
	pub(crate) fn map_index_worker_limit(&self) -> Result<Option<NonZeroUsize>, SettingsIssue> {
		Ok(self.load()?.settings.map_index_worker_limit())
	}

	/// Reads the legacy folder as a picker hint for the native inventory.
	///
	/// # Errors
	///
	/// A corrupt or unreadable original settings file cannot supply a hint.
	pub(crate) fn legacy_root_override(&self) -> Result<Option<PathBuf>, SettingsIssue> {
		self.read_legacy()
			.map(|legacy| legacy.and_then(|legacy| legacy.root))
	}
}

/// Loads current preferences, falling back to legacy choices when needed.
#[tauri::command]
pub(crate) async fn read_settings(app: AppHandle) -> Result<SettingsSnapshot, SettingsIssue> {
	tauri::async_runtime::spawn_blocking(move || SettingsStore::for_app(&app)?.load_or_migrate())
		.await
		.map_err(|_| SettingsIssue::StorageUnavailable)?
}

/// Applies one validated preference change to the latest stored settings.
#[tauri::command]
pub(crate) async fn change_setting(
	app: AppHandle,
	change: SettingChange,
) -> Result<SettingsSnapshot, SettingsIssue> {
	tauri::async_runtime::spawn_blocking(move || SettingsStore::for_app(&app)?.change(change))
		.await
		.map_err(|_| SettingsIssue::StorageUnavailable)?
}

/// Replaces corrupt settings after an explicit recovery request.
#[tauri::command]
pub(crate) async fn recover_settings(app: AppHandle) -> Result<SettingsSnapshot, SettingsIssue> {
	tauri::async_runtime::spawn_blocking(move || SettingsStore::for_app(&app)?.recover())
		.await
		.map_err(|_| SettingsIssue::StorageUnavailable)?
}
