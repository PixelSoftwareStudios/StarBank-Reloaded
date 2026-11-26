from pathlib import Path
import json

source = Path(__file__).resolve().with_name("source")
if (source / ".git").exists():
    raise SystemExit("Use a disposable git archive in the source folder.")
tauri = source / "apps/starbank-desktop/src-tauri"


def replace(path, old, new):
    text = path.read_text()
    assert text.count(old) == 1, (path.name, old)
    path.write_text(text.replace(old, new, 1))


def span(path, start, end, new):
    text = path.read_text()
    begin = text.index(start)
    limit = text.index(end, begin)
    path.write_text(text[:begin] + new + text[limit:])


desktop = tauri / "src/desktop.rs"
replace(
    desktop, "pub(crate) fn run() -> tauri::Result<()> {",
    """pub(crate) fn benchmark_root() -> std::path::PathBuf {
	std::env::var_os("STARBANK_BENCH_ROOT")
		.map(std::path::PathBuf::from)
		.expect("isolated benchmark root")
}

pub(crate) fn run() -> tauri::Result<()> {""",
)
replace(
    desktop, "let cache_path = app.path().app_cache_dir().ok().and_then(|directory| {",
    'let cache_path = Some(benchmark_root().join("Cache")).and_then(|directory| {',
)
span(
    desktop, "\t\t\tlet root_record = app", "\t\t\tapp.manage(",
    '\t\t\tlet root_record = Some(benchmark_root().join("Config").join(INVENTORY_ROOT));\n',
)
replace(desktop, "WebviewWindowBuilder::from_config(app, config)?",
        "WebviewWindowBuilder::from_config(app, config)?\n\t\t\t\t.incognito(true)")

span(
    tauri / "src/settings.rs", "\tpub(crate) fn for_app(app: &AppHandle)",
    "\n\t/// Reads the two preferences",
    """	pub(crate) fn for_app(_app: &AppHandle) -> Result<Self, SettingsIssue> {
		let root = crate::desktop::benchmark_root();
		Ok(Self::new(
			root.join("Config"),
			root.join(LEGACY_DIRECTORY).join(SETTINGS_FILE_NAME),
		))
	}
""",
)
replace(
    tauri / "src/backup.rs",
    "pub(crate) fn backup_folder(app: &AppHandle, create: bool) -> Result<PathBuf, CommandError> {\n\tlet data = app.path().app_data_dir().map_err(write_rejected)?;",
    'pub(crate) fn backup_folder(_app: &AppHandle, create: bool) -> Result<PathBuf, CommandError> {\n\tlet data = crate::desktop::benchmark_root().join("Data");',
)
replace(
    tauri / "src/inventory.rs",
    "use starbank_map_index::{Cancellation, known_cache_roots};",
    "use starbank_map_index::Cancellation;",
)
replace(
    tauri / "src/inventory.rs",
    "self.scan_with_progress(mode, &known_cache_roots(), |update| {",
    'self.scan_with_progress(mode, &[crate::desktop::benchmark_root().join("Archives")], |update| {',
)
replace(
    tauri / "src/inventory/scan.rs",
    "PathError, PathSelection, ScanLimits, get_sc2_root_path, inventory_banks, is_valid_bank_path,",
    "PathError, PathSelection, ScanLimits, inventory_banks, is_valid_bank_path,",
)
replace(
    tauri / "src/inventory/scan.rs",
    "None => get_sc2_root_path(BANK_SCAN_LIMITS).map(|detected| detected.selection),",
    'None => manual_selection(&crate::desktop::benchmark_root().join("Inventory")),',
)
conf = tauri / "tauri.conf.json"
config = json.loads(conf.read_text())
config["identifier"] = "org.starbank-reloaded.perf-scratch"
conf.write_text(json.dumps(config, indent=2) + "\n")
(tauri / "tauri.macos.conf.json").write_text("{}\n")
