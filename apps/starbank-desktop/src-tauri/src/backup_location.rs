use std::{
	fs, io,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use starbank_fs::{BackupDirectory, NativeBackupCatalog};
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::{
	commands::{CommandError, DesktopEngine},
	export::can_export_bank,
	save::write_rejected,
	settings::{SettingsIssue, SettingsStore},
};

const BACKUP_PREFERENCES: &str = "backup-preferences.json";
const BACKUP_DIRECTORY: &str = "Backups";
const RECORD_VERSION: u32 = 1;
const MAX_CUSTOM_FOLDERS: usize = 8;
const MAX_KEEP_NEWEST: usize = 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ManagedBackupFolder {
	Default(PathBuf),
	Selected(BackupDirectory),
}

impl ManagedBackupFolder {
	pub(crate) fn path(&self) -> &Path {
		match self {
			Self::Default(path) => path,
			Self::Selected(folder) => folder.path(),
		}
	}

	pub(crate) fn authorize(&self, catalog: &mut NativeBackupCatalog) {
		catalog.set_managed_directory(match self {
			Self::Default(_) => None,
			Self::Selected(folder) => Some(folder.clone()),
		});
	}
}

pub(crate) struct ManagedBackupHistory {
	pub(crate) folders: Vec<ManagedBackupFolder>,
	pub(crate) unavailable: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupPreferences {
	version: u32,
	folders: Vec<BackupDirectory>,
	selected: Option<usize>,
	keep_newest: Option<NonZeroUsize>,
}

impl Default for BackupPreferences {
	fn default() -> Self {
		Self {
			version: RECORD_VERSION,
			folders: Vec::new(),
			selected: None,
			keep_newest: None,
		}
	}
}

impl BackupPreferences {
	fn decode(bytes: Option<&[u8]>) -> Result<Self, SettingsIssue> {
		let Some(bytes) = bytes else {
			return Ok(Self::default());
		};
		let preferences: Self =
			serde_json::from_slice(bytes).map_err(|_| SettingsIssue::InvalidSettings)?;
		if preferences.version != RECORD_VERSION {
			return Err(SettingsIssue::UnsupportedVersion);
		}
		if preferences.folders.len() > MAX_CUSTOM_FOLDERS
			|| preferences
				.selected
				.is_some_and(|id| id >= preferences.folders.len())
			|| preferences
				.keep_newest
				.is_some_and(|count| count.get() > MAX_KEEP_NEWEST)
			|| preferences
				.folders
				.iter()
				.enumerate()
				.any(|(index, folder)| {
					preferences.folders[..index]
						.iter()
						.any(|previous| previous.path() == folder.path())
				}) {
			return Err(SettingsIssue::InvalidSettings);
		}
		Ok(preferences)
	}

	fn select(&mut self, folder: BackupDirectory) -> Result<(), SettingsIssue> {
		if let Some(index) = self
			.folders
			.iter()
			.position(|known| known.path() == folder.path())
		{
			self.folders[index] = folder;
			self.selected = Some(index);
		} else {
			if self.folders.len() == MAX_CUSTOM_FOLDERS {
				return Err(SettingsIssue::SettingsTooLarge);
			}
			self.selected = Some(self.folders.len());
			self.folders.push(folder);
		}
		Ok(())
	}

	fn selected(&self) -> Option<&BackupDirectory> {
		self.selected.and_then(|index| self.folders.get(index))
	}

	fn view(&self) -> BackupPreferencesView {
		let location = match self.selected() {
			None => BackupLocationView::Default,
			Some(folder) => {
				let name = folder.path().file_name().map_or_else(
					|| "Selected folder".to_owned(),
					|name| name.to_string_lossy().into_owned(),
				);
				if folder.validate().is_ok() {
					BackupLocationView::Selected { name }
				} else {
					BackupLocationView::NeedsAuthorization { name }
				}
			}
		};
		BackupPreferencesView {
			location,
			keep_newest: self.keep_newest,
		}
	}
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupPreferencesView {
	location: BackupLocationView,
	keep_newest: Option<NonZeroUsize>,
}

#[derive(Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum BackupLocationView {
	Default,
	Selected { name: String },
	NeedsAuthorization { name: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackupPreferencesIssue {
	Unavailable,
	UnsafeFolder,
	InvalidRetention,
	RecordInvalid,
	RecordChanged,
	FolderLimit,
	DialogBusy,
}

impl From<SettingsIssue> for BackupPreferencesIssue {
	fn from(issue: SettingsIssue) -> Self {
		match issue {
			SettingsIssue::InvalidSettings
			| SettingsIssue::InvalidJson
			| SettingsIssue::UnsupportedVersion => Self::RecordInvalid,
			SettingsIssue::SettingsChanged => Self::RecordChanged,
			SettingsIssue::SettingsTooLarge => Self::FolderLimit,
			SettingsIssue::StorageUnavailable => Self::Unavailable,
		}
	}
}

fn read(app: &AppHandle) -> Result<BackupPreferences, SettingsIssue> {
	let snapshot = SettingsStore::for_app(app)?.read_record(BACKUP_PREFERENCES)?;
	BackupPreferences::decode(snapshot.as_ref().map(starbank_fs::FileSnapshot::bytes))
}

fn update(
	app: &AppHandle,
	change: impl FnOnce(&mut BackupPreferences) -> Result<(), SettingsIssue>,
) -> Result<BackupPreferencesView, BackupPreferencesIssue> {
	Ok(
		SettingsStore::for_app(app)?.update_record(BACKUP_PREFERENCES, |original| {
			let mut settings = BackupPreferences::decode(original)?;
			change(&mut settings)?;
			let bytes =
				serde_json::to_vec(&settings).map_err(|_| SettingsIssue::InvalidSettings)?;
			Ok((bytes, settings.view()))
		})?,
	)
}

fn available() -> Result<(), BackupPreferencesIssue> {
	if can_export_bank() {
		Ok(())
	} else {
		Err(BackupPreferencesIssue::Unavailable)
	}
}

#[tauri::command]
pub(crate) async fn read_backup_preferences(
	app: AppHandle,
) -> Result<BackupPreferencesView, BackupPreferencesIssue> {
	available()?;
	tauri::async_runtime::spawn_blocking(move || Ok(read(&app)?.view()))
		.await
		.map_err(|_| BackupPreferencesIssue::Unavailable)?
}

#[tauri::command]
pub(crate) async fn choose_backup_location(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
) -> Result<Option<BackupPreferencesView>, BackupPreferencesIssue> {
	available()?;
	let _dialog = engine
		.dialog()
		.map_err(|_| BackupPreferencesIssue::DialogBusy)?;
	let app = window.app_handle().clone();
	let Some(choice) = rfd::AsyncFileDialog::new()
		.set_title("Choose Managed Backup Folder")
		.set_parent(&window)
		.pick_folder()
		.await
	else {
		return Ok(None);
	};
	let path = choice.path().to_path_buf();
	tauri::async_runtime::spawn_blocking(move || {
		let folder =
			BackupDirectory::capture(&path).map_err(|_| BackupPreferencesIssue::UnsafeFolder)?;
		update(&app, |settings| settings.select(folder)).map(Some)
	})
	.await
	.map_err(|_| BackupPreferencesIssue::Unavailable)?
}

#[tauri::command]
pub(crate) async fn use_default_backup_location(
	app: AppHandle,
	engine: State<'_, DesktopEngine>,
) -> Result<BackupPreferencesView, BackupPreferencesIssue> {
	available()?;
	let _dialog = engine
		.dialog()
		.map_err(|_| BackupPreferencesIssue::DialogBusy)?;
	tauri::async_runtime::spawn_blocking(move || {
		update(&app, |settings| {
			settings.selected = None;
			Ok(())
		})
	})
	.await
	.map_err(|_| BackupPreferencesIssue::Unavailable)?
}

#[tauri::command]
pub(crate) async fn set_backup_retention(
	app: AppHandle,
	keep_newest: Option<usize>,
) -> Result<BackupPreferencesView, BackupPreferencesIssue> {
	available()?;
	let keep_newest = validated_retention(keep_newest)?;
	tauri::async_runtime::spawn_blocking(move || {
		update(&app, |settings| {
			settings.keep_newest = keep_newest;
			Ok(())
		})
	})
	.await
	.map_err(|_| BackupPreferencesIssue::Unavailable)?
}

fn validated_retention(
	count: Option<usize>,
) -> Result<Option<NonZeroUsize>, BackupPreferencesIssue> {
	count
		.map(|value| {
			NonZeroUsize::new(value)
				.filter(|count| count.get() <= MAX_KEEP_NEWEST)
				.ok_or(BackupPreferencesIssue::InvalidRetention)
		})
		.transpose()
}

pub(crate) fn backup_folder(
	app: &AppHandle,
	create: bool,
) -> Result<ManagedBackupFolder, CommandError> {
	write_folder(app, &read(app).map_err(write_rejected)?, create)
}

pub(crate) fn retention_scope(
	app: &AppHandle,
) -> Result<(ManagedBackupFolder, Option<NonZeroUsize>), CommandError> {
	let settings = read(app).map_err(write_rejected)?;
	Ok((write_folder(app, &settings, false)?, settings.keep_newest))
}

fn write_folder(
	app: &AppHandle,
	settings: &BackupPreferences,
	create: bool,
) -> Result<ManagedBackupFolder, CommandError> {
	if let Some(folder) = settings.selected() {
		folder
			.validate()
			.map_err(|_| write_rejected("Choose the managed backup folder again in Settings."))?;
		Ok(ManagedBackupFolder::Selected(folder.clone()))
	} else {
		default_folder(app, create).map(ManagedBackupFolder::Default)
	}
}

pub(crate) fn backup_history(app: &AppHandle) -> ManagedBackupHistory {
	let mut folders = Vec::new();
	let mut unavailable = 0;
	match default_folder(app, false) {
		Ok(path) => folders.push(ManagedBackupFolder::Default(path)),
		Err(_) => unavailable += 1,
	}
	match read(app) {
		Ok(settings) => {
			for folder in settings.folders {
				if folders.iter().all(|known| known.path() != folder.path()) {
					folders.push(ManagedBackupFolder::Selected(folder));
				}
			}
		}
		Err(_) => unavailable += 1,
	}
	ManagedBackupHistory {
		folders,
		unavailable,
	}
}

fn default_folder(app: &AppHandle, create: bool) -> Result<PathBuf, CommandError> {
	let data = app.path().app_data_dir().map_err(write_rejected)?;
	let folder = data.join(BACKUP_DIRECTORY);
	if create {
		ensure_folder(&data)?;
		ensure_folder(&folder)?;
	}
	Ok(folder)
}

fn ensure_folder(path: &Path) -> Result<(), CommandError> {
	#[cfg(unix)]
	let mut builder = fs::DirBuilder::new();
	#[cfg(not(unix))]
	let builder = fs::DirBuilder::new();
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt as _;
		builder.mode(0o700);
	}
	match builder.create(path) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
			if fs::symlink_metadata(path).map_err(write_rejected)?.is_dir() {
				Ok(())
			} else {
				Err(write_rejected("Backup location is not a local directory."))
			}
		}
		Err(error) => Err(write_rejected(error)),
	}
}

#[cfg(test)]
mod tests;
