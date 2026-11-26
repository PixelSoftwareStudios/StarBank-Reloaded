use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{SavePreferences, SettingsIssue};

const SETTINGS_VERSION: u32 = 1;
const INITIAL_PANE_WIDTH: f64 = 28.0;
const MIN_PANE_WIDTH: f64 = 20.0;
const MAX_PANE_WIDTH: f64 = 50.0;
const MAX_LEGACY_PATH_UNITS: usize = 32_767;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AppSettings {
	version: u32,
	hide_blizzard_maps: bool,
	hide_maps_without_banks: bool,
	auto_resign_on_save: bool,
	theme: Theme,
	density: Density,
	text_size: TextSize,
	map_pane_width: f64,
	sections_layout: SectionsLayout,
	show_breadcrumbs: bool,
	wrap_raw: bool,
	group_numbers: bool,
	automatic_backup: AutomaticBackup,
}

impl Default for AppSettings {
	fn default() -> Self {
		Self {
			version: SETTINGS_VERSION,
			hide_blizzard_maps: true,
			hide_maps_without_banks: true,
			auto_resign_on_save: true,
			theme: Theme::System,
			density: Density::Compact,
			text_size: TextSize::Normal,
			map_pane_width: INITIAL_PANE_WIDTH,
			sections_layout: SectionsLayout::Separate,
			show_breadcrumbs: true,
			wrap_raw: false,
			group_numbers: true,
			automatic_backup: AutomaticBackup::FirstOverwrite,
		}
	}
}

impl AppSettings {
	pub(super) fn save_preferences(self) -> SavePreferences {
		SavePreferences {
			auto_resign_on_save: self.auto_resign_on_save,
			automatic_backup: self.automatic_backup,
		}
	}

	pub(super) fn decode(bytes: &[u8]) -> Result<Self, SettingsIssue> {
		let value = parse_object(bytes)?;
		if value
			.get("version")
			.and_then(Value::as_f64)
			.is_some_and(|version| version.fract() == 0.0 && version != f64::from(SETTINGS_VERSION))
		{
			return Err(SettingsIssue::UnsupportedVersion);
		}

		// Deserialize the original bytes to reject duplicate preference fields.
		let settings: Self =
			serde_json::from_slice(bytes).map_err(|_| SettingsIssue::InvalidSettings)?;
		validate_width(settings.map_pane_width)?;
		Ok(settings)
	}

	pub(super) fn apply(&mut self, change: SettingChange) -> Result<(), SettingsIssue> {
		match change {
			SettingChange::HideBlizzardMaps(value) => self.hide_blizzard_maps = value,
			SettingChange::HideMapsWithoutBanks(value) => self.hide_maps_without_banks = value,
			SettingChange::AutoResignOnSave(value) => self.auto_resign_on_save = value,
			SettingChange::Theme(value) => self.theme = value,
			SettingChange::Density(value) => self.density = value,
			SettingChange::TextSize(value) => self.text_size = value,
			SettingChange::MapPaneWidth(value) => {
				validate_width(value)?;
				self.map_pane_width = value;
			}
			SettingChange::SectionsLayout(value) => self.sections_layout = value,
			SettingChange::ShowBreadcrumbs(value) => self.show_breadcrumbs = value,
			SettingChange::WrapRaw(value) => self.wrap_raw = value,
			SettingChange::GroupNumbers(value) => self.group_numbers = value,
			SettingChange::AutomaticBackup(value) => self.automatic_backup = value,
		}
		Ok(())
	}
}

/// A single preference update from the shared settings UI.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(
	tag = "key",
	content = "value",
	rename_all = "camelCase",
	deny_unknown_fields
)]
pub(crate) enum SettingChange {
	HideBlizzardMaps(bool),
	HideMapsWithoutBanks(bool),
	AutoResignOnSave(bool),
	Theme(Theme),
	Density(Density),
	TextSize(TextSize),
	MapPaneWidth(f64),
	SectionsLayout(SectionsLayout),
	ShowBreadcrumbs(bool),
	WrapRaw(bool),
	GroupNumbers(bool),
	AutomaticBackup(AutomaticBackup),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Theme {
	System,
	Light,
	Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Density {
	Compact,
	Comfortable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TextSize {
	Small,
	Normal,
	Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SectionsLayout {
	Separate,
	Unified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AutomaticBackup {
	FirstOverwrite,
	EveryOverwrite,
	Off,
}

#[derive(Debug)]
pub(super) struct LegacyImport {
	pub(super) settings: AppSettings,
	pub(super) root: Option<PathBuf>,
}

#[derive(Deserialize)]
struct LegacySettings {
	hide_blizzard_maps: Option<bool>,
	hide_maps_without_banks: Option<bool>,
	auto_resign_on_save: Option<bool>,
	sc2_root_override: Option<String>,
}

impl LegacyImport {
	pub(super) fn decode(bytes: &[u8]) -> Result<Self, SettingsIssue> {
		if parse_object(bytes)?.get("version").is_some() {
			return Err(SettingsIssue::InvalidSettings);
		}
		let legacy: LegacySettings =
			serde_json::from_slice(bytes).map_err(|_| SettingsIssue::InvalidSettings)?;
		let defaults = AppSettings::default();
		let settings = AppSettings {
			hide_blizzard_maps: legacy
				.hide_blizzard_maps
				.unwrap_or(defaults.hide_blizzard_maps),
			hide_maps_without_banks: legacy
				.hide_maps_without_banks
				.unwrap_or(defaults.hide_maps_without_banks),
			auto_resign_on_save: legacy
				.auto_resign_on_save
				.unwrap_or(defaults.auto_resign_on_save),
			..defaults
		};
		let root = legacy.sc2_root_override.filter(|path| !path.is_empty());
		if let Some(path) = root.as_deref()
			&& (path.contains('\0')
				|| path.encode_utf16().take(MAX_LEGACY_PATH_UNITS + 1).count()
					> MAX_LEGACY_PATH_UNITS)
		{
			return Err(SettingsIssue::InvalidSettings);
		}
		Ok(Self {
			settings,
			root: root.map(PathBuf::from),
		})
	}
}

fn parse_object(bytes: &[u8]) -> Result<Value, SettingsIssue> {
	let value: Value = serde_json::from_slice(bytes).map_err(|_| SettingsIssue::InvalidJson)?;
	if value.is_object() {
		Ok(value)
	} else {
		Err(SettingsIssue::InvalidSettings)
	}
}

fn validate_width(width: f64) -> Result<(), SettingsIssue> {
	if (MIN_PANE_WIDTH..=MAX_PANE_WIDTH).contains(&width) {
		Ok(())
	} else {
		Err(SettingsIssue::InvalidSettings)
	}
}
