use std::{collections::BTreeMap, num::NonZeroUsize, path::PathBuf, sync::Mutex};

use serde::Serialize;
use starbank_map_index::{Cancellation, known_cache_roots};
use tauri::{AppHandle, Manager as _, State, WebviewWindow, ipc::Channel};

use crate::commands::{self, CommandError, DesktopEngine, OpenedBank};
use crate::settings::SettingsStore;

mod authority;
mod backup_status;
mod batch;
mod progress;
mod projection;
mod root_record;
mod scan;

pub(crate) use authority::BankAuthority;
use authority::is_native_disk_path;
pub(crate) use backup_status::InventoryBackupStatus;
use batch::MapMembers;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
pub(crate) use batch::scan_batch_fixture;
pub(crate) use batch::{InventoryBatch, InventoryBatchMember};
pub(crate) use progress::InventoryProgress;
use progress::InventoryProgressPublisher;
use projection::OpaqueIds;
pub(crate) use projection::{InventoryReason, InventorySnapshot, InventoryStatus};
use root_record::{ManualRoot, RootRecordStore};
use scan::{MapCache, MapScanOptions, check_cancelled, collect_inventory};

const PICK_FOLDER_TITLE: &str = "Select SC2 Bank Folder";
const OPEN_REQUEST: &str = "open";
const MAX_GENERATION_DIGITS: usize = 20;

#[derive(Default)]
pub(crate) struct InventoryService {
	state: Mutex<InventoryState>,
	backup_status_scan: Mutex<()>,
	cache_path: Option<PathBuf>,
	starting_directory: Option<PathBuf>,
	root_record: Option<RootRecordStore>,
}

#[derive(Default)]
struct InventoryState {
	manual_root: Option<ManualRoot>,
	root_record_unavailable: bool,
	active: Option<Cancellation>,
	generation: u64,
	banks: BTreeMap<String, BankAuthority>,
	map_members: MapMembers,
	ids: OpaqueIds,
}

enum ScanMode {
	Refresh,
	RebuildMapIndex,
	Manual(PathBuf),
	Automatic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InventoryError {
	Busy,
	Cancelled,
	Unavailable,
	UnknownBank,
	UnknownMap,
	StaleBank,
	MapIndexRebuildFailed,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum InventoryOpenError {
	Inventory(InventoryError),
	Engine(CommandError),
}

impl InventoryService {
	pub(crate) fn with_cache_path(cache_path: Option<PathBuf>) -> Self {
		Self {
			cache_path,
			..Self::default()
		}
	}

	pub(crate) fn with_root_record(mut self, path: Option<PathBuf>) -> Self {
		if let Some(path) = path {
			let store = RootRecordStore::new(path);
			if let Ok(state) = self.state.get_mut() {
				match store.load() {
					Ok(root) => state.manual_root = root,
					Err(_) => state.root_record_unavailable = true,
				}
			}
			self.root_record = Some(store);
		}
		self
	}

	pub(crate) fn with_starting_directory(mut self, directory: Option<PathBuf>) -> Self {
		self.starting_directory = directory.filter(|path| is_native_disk_path(path));
		self
	}

	fn cancel(&self, generation: u64) -> Result<(), InventoryError> {
		let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		if state.generation == generation
			&& let Some(active) = &state.active
		{
			active.cancel();
		}
		Ok(())
	}

	fn authorize_bank(&self, id: &str) -> Result<BankAuthority, InventoryError> {
		if !OpaqueIds::accepts_bank_id(id) {
			return Err(InventoryError::UnknownBank);
		}
		let authority = {
			let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
			if state.active.is_some() {
				return Err(InventoryError::Busy);
			}
			state
				.banks
				.get(id)
				.cloned()
				.ok_or(InventoryError::UnknownBank)?
		};
		authority.validate()?;
		Ok(authority)
	}

	fn picker_directory(&self) -> Result<Option<PathBuf>, InventoryError> {
		let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		if state.active.is_some() {
			return Err(InventoryError::Busy);
		}
		Ok(state
			.manual_root
			.as_ref()
			.map(|root| &root.path)
			.filter(|path| is_native_disk_path(path))
			.cloned()
			.or_else(|| self.starting_directory.clone()))
	}

	fn scan(
		&self,
		mode: ScanMode,
		progress: Channel<InventoryProgress>,
		app: &AppHandle,
	) -> Result<InventorySnapshot, InventoryError> {
		let max_workers = SettingsStore::for_app(app)
			.and_then(|store| store.map_index_worker_limit())
			.unwrap_or(None);
		self.scan_with_workers(mode, &known_cache_roots(), max_workers, |update| {
			// A closed webview must not interrupt a scan or its native state.
			let _ = progress.send(update);
		})
	}

	#[cfg(test)]
	fn scan_with_roots(
		&self,
		mode: ScanMode,
		map_roots: &[PathBuf],
	) -> Result<InventorySnapshot, InventoryError> {
		self.scan_with_progress(mode, map_roots, |_| {})
	}

	#[cfg(test)]
	fn scan_with_progress(
		&self,
		mode: ScanMode,
		map_roots: &[PathBuf],
		notify: impl FnMut(InventoryProgress),
	) -> Result<InventorySnapshot, InventoryError> {
		self.scan_with_workers(mode, map_roots, None, notify)
	}

	fn scan_with_workers(
		&self,
		mode: ScanMode,
		map_roots: &[PathBuf],
		max_workers: Option<NonZeroUsize>,
		notify: impl FnMut(InventoryProgress),
	) -> Result<InventorySnapshot, InventoryError> {
		let rebuilding = matches!(mode, ScanMode::RebuildMapIndex);
		let cache_path = self.cache_path.as_deref();
		let cache = if rebuilding {
			MapCache::Rebuild(cache_path.ok_or(InventoryError::MapIndexRebuildFailed)?)
		} else {
			MapCache::Reuse(cache_path)
		};
		let lease = self.start_scan(mode)?;
		let mut progress = InventoryProgressPublisher::new(lease.generation, notify);
		let ((mut snapshot, banks), accepted_root, rebuilt_cache) = collect_inventory(
			lease.root.as_ref(),
			map_roots,
			MapScanOptions { cache, max_workers },
			&lease.ids,
			lease.generation,
			&lease.cancellation,
			&mut progress,
		)?;
		let mut state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		check_cancelled(&lease.cancellation)?;
		if rebuilding && snapshot.status == InventoryStatus::Ready {
			// The lease serializes cancellation with the final replacement.
			rebuilt_cache
				.filter(|_| !snapshot.incomplete)
				.ok_or(InventoryError::MapIndexRebuildFailed)?
				.publish()
				.map_err(|_| InventoryError::MapIndexRebuildFailed)?;
		}
		let record_value = match lease.change {
			RootChange::Keep => None,
			RootChange::Manual => accepted_root.as_ref().map(Some),
			RootChange::Automatic => Some(None),
		};
		if let (Some(store), Some(manual)) = (&self.root_record, record_value) {
			state.root_record_unavailable = store.save(manual).is_err();
		}
		match lease.change {
			RootChange::Keep => {
				if accepted_root.is_some() {
					state.manual_root = accepted_root;
				}
			}
			RootChange::Manual => {
				// Keep an invalid explicit selection visible so a refresh cannot use a different scope.
				state.manual_root = accepted_root.or_else(|| lease.root.clone());
			}
			RootChange::Automatic => state.manual_root = None,
		}
		snapshot.generation = lease.generation.to_string();
		state.banks = banks;
		state.map_members = snapshot.map_members();
		if state.root_record_unavailable {
			snapshot.incomplete = true;
			snapshot
				.reason
				.get_or_insert(InventoryReason::ConfigurationUnavailable);
		}
		drop(state);
		Ok(snapshot)
	}

	fn start_scan(&self, mode: ScanMode) -> Result<ScanLease<'_>, InventoryError> {
		let mut state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		if state.active.is_some() {
			return Err(InventoryError::Busy);
		}
		// Keep the published root and bank IDs until the scan crosses its final cancellation check.
		let (root, change) = match mode {
			ScanMode::Refresh | ScanMode::RebuildMapIndex => {
				(state.manual_root.clone(), RootChange::Keep)
			}
			ScanMode::Manual(path) => (
				Some(ManualRoot {
					path,
					identity: None,
				}),
				RootChange::Manual,
			),
			ScanMode::Automatic => (None, RootChange::Automatic),
		};
		let cancellation = Cancellation::default();
		state.generation = state
			.generation
			.checked_add(1)
			.ok_or(InventoryError::Unavailable)?;
		state.active = Some(cancellation.clone());
		Ok(ScanLease {
			service: self,
			root,
			change,
			ids: state.ids.clone(),
			generation: state.generation,
			cancellation,
		})
	}
}

enum RootChange {
	Keep,
	Manual,
	Automatic,
}

struct ScanLease<'a> {
	service: &'a InventoryService,
	root: Option<ManualRoot>,
	change: RootChange,
	ids: OpaqueIds,
	generation: u64,
	cancellation: Cancellation,
}

impl Drop for ScanLease<'_> {
	fn drop(&mut self) {
		if let Ok(mut state) = self.service.state.lock() {
			state.active = None;
		}
	}
}

#[tauri::command]
pub(crate) async fn inventory_backup_status(
	window: WebviewWindow,
	generation: String,
) -> Result<Vec<InventoryBackupStatus>, InventoryError> {
	if generation.len() > MAX_GENERATION_DIGITS {
		return Err(InventoryError::Unavailable);
	}
	let generation = generation
		.parse()
		.map_err(|_| InventoryError::Unavailable)?;
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let inventory = app.state::<InventoryService>();
		inventory.recorded_backups(generation, || crate::backup_location::backup_history(&app))
	})
	.await
	.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn refresh_inventory(
	window: WebviewWindow,
	progress: Channel<InventoryProgress>,
) -> Result<InventorySnapshot, InventoryError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		app.state::<InventoryService>()
			.scan(ScanMode::Refresh, progress, &app)
	})
	.await
	.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn rebuild_map_index(
	window: WebviewWindow,
	progress: Channel<InventoryProgress>,
) -> Result<InventorySnapshot, InventoryError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		app.state::<InventoryService>()
			.scan(ScanMode::RebuildMapIndex, progress, &app)
	})
	.await
	.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn use_automatic_inventory(
	window: WebviewWindow,
	progress: Channel<InventoryProgress>,
) -> Result<InventorySnapshot, InventoryError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		app.state::<InventoryService>()
			.scan(ScanMode::Automatic, progress, &app)
	})
	.await
	.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn select_inventory_folder(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
	inventory: State<'_, InventoryService>,
	progress: Channel<InventoryProgress>,
) -> Result<Option<InventorySnapshot>, InventoryError> {
	let _dialog = engine.dialog().map_err(|_| InventoryError::Busy)?;
	let mut dialog = rfd::AsyncFileDialog::new()
		.set_parent(&window)
		.set_title(PICK_FOLDER_TITLE);
	if let Some(path) = inventory.picker_directory()? {
		dialog = dialog.set_directory(path);
	}
	let Some(selection) = dialog.pick_folder().await else {
		return Ok(None);
	};
	let path = selection.path().to_owned();
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		app.state::<InventoryService>()
			.scan(ScanMode::Manual(path), progress, &app)
			.map(Some)
	})
	.await
	.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) fn cancel_inventory(
	inventory: State<'_, InventoryService>,
	generation: String,
) -> Result<(), InventoryError> {
	if generation.len() > MAX_GENERATION_DIGITS {
		return Err(InventoryError::Unavailable);
	}
	let generation = generation
		.parse()
		.map_err(|_| InventoryError::Unavailable)?;
	inventory.cancel(generation)
}

#[tauri::command]
pub(crate) async fn open_bank_id(
	window: WebviewWindow,
	id: String,
	control: String,
) -> Result<OpenedBank, InventoryOpenError> {
	if !commands::is_request(&control, OPEN_REQUEST).map_err(InventoryOpenError::Engine)? {
		return Err(InventoryOpenError::Engine(CommandError::InvalidControl));
	}
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let inventory = app.state::<InventoryService>();
		let authority = inventory
			.authorize_bank(&id)
			.map_err(InventoryOpenError::Inventory)?;
		let engine = app.state::<DesktopEngine>();
		let mut engine = engine
			.engine
			.lock()
			.map_err(|_| InventoryOpenError::Engine(CommandError::Unavailable))?;
		authority
			.validate()
			.map_err(InventoryOpenError::Inventory)?;
		engine
			.open_path(&control, authority.path())
			.map_err(InventoryOpenError::Engine)
	})
	.await
	.map_err(|_| InventoryOpenError::Inventory(InventoryError::Unavailable))?
}

#[cfg(test)]
mod tests;
