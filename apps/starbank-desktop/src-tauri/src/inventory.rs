use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

use serde::Serialize;
use starbank_map_index::{Cancellation, known_cache_roots};
use tauri::{Manager as _, State, WebviewWindow};

use crate::commands::{self, CommandError, DesktopEngine, OpenedBank};

mod authority;
mod projection;
mod root_record;
mod scan;

pub(crate) use authority::BankAuthority;
use authority::is_native_disk_path;
use projection::OpaqueIds;
pub(crate) use projection::{InventoryReason, InventorySnapshot, InventoryStatus};
use root_record::{ManualRoot, RootRecordStore};
use scan::{check_cancelled, collect_inventory};

const PICK_FOLDER_TITLE: &str = "Select SC2 Bank Folder";
const OPEN_REQUEST: &str = "open";

#[derive(Default)]
pub(crate) struct InventoryService {
	state: Mutex<InventoryState>,
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
	ids: OpaqueIds,
}

enum ScanMode {
	Refresh,
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
	StaleBank,
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

	fn refresh(&self) -> Result<InventorySnapshot, InventoryError> {
		self.scan(ScanMode::Refresh)
	}

	fn use_automatic(&self) -> Result<InventorySnapshot, InventoryError> {
		self.scan(ScanMode::Automatic)
	}

	fn cancel(&self) -> Result<(), InventoryError> {
		let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		if let Some(active) = &state.active {
			active.cancel();
		}
		Ok(())
	}

	fn authorize_bank(&self, id: &str) -> Result<BankAuthority, InventoryError> {
		if !OpaqueIds::accepts_bank_id(id) {
			return Err(InventoryError::UnknownBank);
		}
		let authority = self
			.state
			.lock()
			.map_err(|_| InventoryError::Unavailable)?
			.banks
			.get(id)
			.cloned()
			.ok_or(InventoryError::UnknownBank)?;
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

	fn scan(&self, mode: ScanMode) -> Result<InventorySnapshot, InventoryError> {
		self.scan_with_roots(mode, &known_cache_roots())
	}

	fn scan_with_roots(
		&self,
		mode: ScanMode,
		map_roots: &[PathBuf],
	) -> Result<InventorySnapshot, InventoryError> {
		let lease = self.start_scan(mode)?;
		let ((mut snapshot, banks), accepted_root) = collect_inventory(
			lease.root.as_ref(),
			map_roots,
			self.cache_path.as_deref(),
			&lease.ids,
			lease.generation,
			&lease.cancellation,
		)?;
		let mut state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		check_cancelled(&lease.cancellation)?;
		let record_value = match lease.change {
			RootChange::Keep => None,
			RootChange::Manual => accepted_root.as_ref().map(Some),
			RootChange::Automatic => Some(None),
		};
		if let (Some(store), Some(manual)) = (&self.root_record, record_value) {
			state.root_record_unavailable = store.save(manual).is_err();
		}
		if accepted_root.is_some() {
			state.manual_root = accepted_root;
		}
		state.banks = banks;
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
		let change = match mode {
			ScanMode::Refresh => RootChange::Keep,
			ScanMode::Manual(path) => {
				if state.manual_root.as_ref().map(|root| &root.path) != Some(&path) {
					state.banks.clear();
				}
				state.manual_root = Some(ManualRoot {
					path,
					identity: None,
				});
				RootChange::Manual
			}
			ScanMode::Automatic => {
				if state.manual_root.take().is_some() {
					state.banks.clear();
				}
				RootChange::Automatic
			}
		};
		let cancellation = Cancellation::default();
		state.generation = state
			.generation
			.checked_add(1)
			.ok_or(InventoryError::Unavailable)?;
		state.active = Some(cancellation.clone());
		Ok(ScanLease {
			service: self,
			root: state.manual_root.clone(),
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
pub(crate) async fn refresh_inventory(
	window: WebviewWindow,
) -> Result<InventorySnapshot, InventoryError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || app.state::<InventoryService>().refresh())
		.await
		.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn use_automatic_inventory(
	window: WebviewWindow,
) -> Result<InventorySnapshot, InventoryError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || app.state::<InventoryService>().use_automatic())
		.await
		.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn select_inventory_folder(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
	inventory: State<'_, InventoryService>,
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
			.scan(ScanMode::Manual(path))
			.map(Some)
	})
	.await
	.map_err(|_| InventoryError::Unavailable)?
}

#[tauri::command]
pub(crate) fn cancel_inventory(
	inventory: State<'_, InventoryService>,
) -> Result<(), InventoryError> {
	inventory.cancel()
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
