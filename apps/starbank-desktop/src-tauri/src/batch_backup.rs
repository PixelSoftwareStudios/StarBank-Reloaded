use std::{
	path::Path,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
};

use serde::{Deserialize, Serialize};
use starbank_backup::BackupError;
use starbank_fs::{BackupPlacement, NativeBackupCatalog, NativeBackupError, backup_source};
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::{
	backup,
	commands::DesktopEngine,
	inventory::{InventoryBatch, InventoryBatchMember, InventoryError, InventoryService},
	save::confirm,
};

const MAX_BATCH_BANKS: usize = 4_096;
const CONFIRM_TITLE: &str = "Back up these bank files?";

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BatchBackupScope {
	All {},
	Map { id: String },
}

impl BatchBackupScope {
	fn map_id(&self) -> Option<&str> {
		match self {
			Self::All {} => None,
			Self::Map { id } => Some(id),
		}
	}
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BatchBackupPlacement {
	Managed,
	Adjacent,
}

impl BatchBackupPlacement {
	fn describe(self) -> &'static str {
		match self {
			Self::Managed => "The verified copies will be stored in StarBank's Backups folder.",
			Self::Adjacent => "Each verified copy will be stored beside its source bank.",
		}
	}
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BatchBackupError {
	Unsupported,
	Busy,
	Unavailable,
	NoBanks,
	TooManyBanks,
	Inventory(InventoryError),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchBackupReport {
	entries: Vec<BatchEntry>,
	created: usize,
	failed: usize,
	skipped: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchEntry {
	bank_id: String,
	name: String,
	outcome: BatchOutcome,
	backup_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BatchOutcome {
	Created,
	SourceUnavailable,
	Rejected,
	NeedsReview,
	Cancelled,
	InventoryChanged,
}

type ActiveBatch = Mutex<Option<Arc<AtomicBool>>>;

#[derive(Default)]
pub(crate) struct BatchBackupService {
	active: Arc<ActiveBatch>,
}

struct BatchLease {
	active: Arc<ActiveBatch>,
	cancellation: Arc<AtomicBool>,
}

impl BatchBackupService {
	fn begin(&self) -> Result<BatchLease, BatchBackupError> {
		let mut active = self
			.active
			.lock()
			.map_err(|_| BatchBackupError::Unavailable)?;
		if active.is_some() {
			return Err(BatchBackupError::Busy);
		}
		let cancellation = Arc::new(AtomicBool::new(false));
		*active = Some(Arc::clone(&cancellation));
		Ok(BatchLease {
			active: Arc::clone(&self.active),
			cancellation,
		})
	}

	fn cancel(&self) -> Result<(), BatchBackupError> {
		let active = self
			.active
			.lock()
			.map_err(|_| BatchBackupError::Unavailable)?;
		if let Some(cancellation) = &*active {
			cancellation.store(true, Ordering::Release);
		}
		Ok(())
	}
}

impl Drop for BatchLease {
	fn drop(&mut self) {
		if let Ok(mut active) = self.active.lock() {
			*active = None;
		}
	}
}

#[tauri::command]
pub(crate) async fn create_batch_backup(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
	batch: State<'_, BatchBackupService>,
	scope: BatchBackupScope,
	placement: BatchBackupPlacement,
) -> Result<Option<BatchBackupReport>, BatchBackupError> {
	if !cfg!(any(target_os = "macos", target_os = "linux")) {
		return Err(BatchBackupError::Unsupported);
	}
	let lease = batch.begin()?;
	let dialog = engine.dialog().map_err(|_| BatchBackupError::Busy)?;
	let app = window.app_handle().clone();
	let preparation = app.clone();
	let plan = tauri::async_runtime::spawn_blocking(move || {
		preparation
			.state::<InventoryService>()
			.capture_batch(scope.map_id())
	})
	.await
	.map_err(|_| BatchBackupError::Unavailable)?
	.map_err(BatchBackupError::Inventory)?;
	let count = plan.members.len();
	if count == 0 {
		return Err(BatchBackupError::NoBanks);
	}
	if count > MAX_BATCH_BANKS {
		return Err(BatchBackupError::TooManyBanks);
	}
	let description = format!(
		"Back up {count} bank files? {} Existing files stay intact.",
		placement.describe()
	);
	if !confirm(&window, CONFIRM_TITLE, &description).await {
		return Ok(None);
	}
	drop(dialog);

	let report = tauri::async_runtime::spawn_blocking(move || {
		let report = publish(&app, plan, placement, &lease.cancellation);
		drop(lease);
		report
	})
	.await
	.map_err(|_| BatchBackupError::Unavailable)?;
	Ok(Some(report))
}

#[tauri::command]
pub(crate) fn cancel_batch_backup(
	batch: State<'_, BatchBackupService>,
) -> Result<(), BatchBackupError> {
	batch.cancel()
}

fn publish(
	app: &AppHandle,
	plan: InventoryBatch,
	placement: BatchBackupPlacement,
	cancellation: &AtomicBool,
) -> BatchBackupReport {
	let inventory = app.state::<InventoryService>();
	let engine = app.state::<DesktopEngine>();
	let generation = plan.generation;
	let mut managed = None;
	execute(
		plan.members,
		|| {
			if cancellation.load(Ordering::Acquire) {
				Some(BatchOutcome::Cancelled)
			} else if inventory.validate_batch_generation(generation).is_err() {
				Some(BatchOutcome::InventoryChanged)
			} else {
				None
			}
		},
		|member| {
			let Ok(mut session) = engine.engine.lock() else {
				return (BatchOutcome::Rejected, None);
			};
			// Recheck after the engine lock, before authorizing any backup output.
			if cancellation.load(Ordering::Acquire) {
				return (BatchOutcome::Cancelled, None);
			}
			if inventory.validate_batch_generation(generation).is_err() {
				return (BatchOutcome::InventoryChanged, None);
			}
			if member.authority.validate().is_err() {
				return (BatchOutcome::SourceUnavailable, None);
			}
			let Ok(catalog) = backup::catalog(&mut session) else {
				return (BatchOutcome::Rejected, None);
			};
			let destination = match placement {
				BatchBackupPlacement::Adjacent => {
					catalog.set_managed_directory(None);
					BackupPlacement::Adjacent
				}
				BatchBackupPlacement::Managed => {
					match managed
						.get_or_insert_with(|| crate::backup_location::backup_folder(app, true))
					{
						Ok(folder) => {
							folder.authorize(catalog);
							BackupPlacement::Managed(folder.path())
						}
						Err(_) => return (BatchOutcome::Rejected, None),
					}
				}
			};
			create_one(catalog, member, destination)
		},
	)
}

fn execute(
	members: Vec<InventoryBatchMember>,
	mut stop: impl FnMut() -> Option<BatchOutcome>,
	mut create: impl FnMut(&InventoryBatchMember) -> (BatchOutcome, Option<String>),
) -> BatchBackupReport {
	let mut report = BatchBackupReport {
		entries: Vec::with_capacity(members.len()),
		created: 0,
		failed: 0,
		skipped: 0,
	};
	let mut stopped = None;
	for member in members {
		let (outcome, backup_name) = match stopped.or_else(&mut stop) {
			Some(reason) => (reason, None),
			None => create(&member),
		};
		match outcome {
			BatchOutcome::Created => report.created += 1,
			BatchOutcome::Cancelled | BatchOutcome::InventoryChanged => {
				stopped = Some(outcome);
				report.skipped += 1;
			}
			BatchOutcome::SourceUnavailable
			| BatchOutcome::Rejected
			| BatchOutcome::NeedsReview => {
				report.failed += 1;
			}
		}
		report.entries.push(BatchEntry {
			bank_id: member.id,
			name: member.name,
			outcome,
			backup_name,
		});
	}
	report
}

fn create_one(
	catalog: &mut NativeBackupCatalog,
	member: &InventoryBatchMember,
	placement: BackupPlacement<'_>,
) -> (BatchOutcome, Option<String>) {
	let Ok(source) = backup_source(member.authority.path()) else {
		return (BatchOutcome::SourceUnavailable, None);
	};
	match catalog.create(&source, placement) {
		Ok(id) => (BatchOutcome::Created, leaf_name(id.path())),
		Err(NativeBackupError::Policy(error)) => match *error {
			BackupError::MissingFile | BackupError::TargetChanged => {
				(BatchOutcome::SourceUnavailable, None)
			}
			BackupError::UncertainBackup { path, .. } => {
				(BatchOutcome::NeedsReview, leaf_name(&path))
			}
			BackupError::Metadata { record, .. } => {
				(BatchOutcome::NeedsReview, leaf_name(record.id().path()))
			}
			_ => (BatchOutcome::Rejected, None),
		},
		Err(_) => (BatchOutcome::Rejected, None),
	}
}

fn leaf_name(path: &Path) -> Option<String> {
	path.file_name()
		.map(|value| value.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests;
