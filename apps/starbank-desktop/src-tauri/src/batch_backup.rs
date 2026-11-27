use std::{
	num::NonZeroU32,
	path::Path,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
};

use serde::{Deserialize, Serialize};
use starbank_backup::{BackupError, RecordFailure};
use starbank_fs::{
	BackupPlacement, CommitIssue, NativeBackupCatalog, NativeBackupError, StorageError,
	backup_source,
};
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::{
	backup,
	commands::DesktopEngine,
	inventory::{InventoryBatch, InventoryBatchMember, InventoryError, InventoryService},
	save::confirm,
};

mod review;

use review::{
	BatchBackupReview, BatchReviewTarget, CompletedReview, ReviewEvidence, ReviewKind, ReviewRows,
	SharedDirectories,
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
	InvalidReview,
	Inventory(InventoryError),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchBackupReport {
	run_id: Option<NonZeroU32>,
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

#[derive(Default)]
struct BatchState {
	active: Option<Arc<AtomicBool>>,
	last_run: u32,
	completed: Option<CompletedReview>,
}

#[derive(Default)]
pub(crate) struct BatchBackupService {
	state: Arc<Mutex<BatchState>>,
}

struct BatchLease {
	state: Arc<Mutex<BatchState>>,
	cancellation: Arc<AtomicBool>,
	run_id: NonZeroU32,
}

impl BatchBackupService {
	fn begin(&self) -> Result<BatchLease, BatchBackupError> {
		let mut state = self
			.state
			.lock()
			.map_err(|_| BatchBackupError::Unavailable)?;
		if state.active.is_some() {
			return Err(BatchBackupError::Busy);
		}
		let run_id = state
			.last_run
			.checked_add(1)
			.and_then(NonZeroU32::new)
			.ok_or(BatchBackupError::Unavailable)?;
		let cancellation = Arc::new(AtomicBool::new(false));
		state.last_run = run_id.get();
		state.active = Some(Arc::clone(&cancellation));
		Ok(BatchLease {
			state: Arc::clone(&self.state),
			cancellation,
			run_id,
		})
	}

	fn cancel(&self) -> Result<(), BatchBackupError> {
		let state = self
			.state
			.lock()
			.map_err(|_| BatchBackupError::Unavailable)?;
		if let Some(cancellation) = &state.active {
			cancellation.store(true, Ordering::Release);
		}
		Ok(())
	}

	fn review(
		&self,
		owner: &str,
		target: BatchReviewTarget,
	) -> Result<BatchBackupReview, BatchBackupError> {
		let state = self
			.state
			.lock()
			.map_err(|_| BatchBackupError::Unavailable)?;
		state
			.completed
			.as_ref()
			.ok_or(BatchBackupError::InvalidReview)?
			.read(owner, target)
	}
}

impl BatchLease {
	fn complete(
		self,
		owner: String,
		mut batch: ExecutedBatch,
	) -> Result<BatchBackupReport, BatchBackupError> {
		let mut state = self
			.state
			.lock()
			.map_err(|_| BatchBackupError::Unavailable)?;
		batch.report.run_id = (!batch.reviews.is_empty()).then_some(self.run_id);
		state.completed = batch.report.run_id.map(|run_id| CompletedReview {
			run_id,
			owner,
			rows: batch.reviews,
		});
		Ok(batch.report)
	}
}

impl Drop for BatchLease {
	fn drop(&mut self) {
		if let Ok(mut state) = self.state.lock() {
			state.active = None;
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

	let owner = window.label().to_owned();
	tauri::async_runtime::spawn_blocking(move || {
		let batch = publish(&app, plan, placement, &lease.cancellation);
		lease.complete(owner, batch)
	})
	.await
	.map_err(|_| BatchBackupError::Unavailable)?
	.map(Some)
}

#[tauri::command]
pub(crate) fn cancel_batch_backup(
	batch: State<'_, BatchBackupService>,
) -> Result<(), BatchBackupError> {
	batch.cancel()
}

#[tauri::command]
pub(crate) fn review_batch_backup(
	window: WebviewWindow,
	batch: State<'_, BatchBackupService>,
	target: BatchReviewTarget,
) -> Result<BatchBackupReview, BatchBackupError> {
	batch.review(window.label(), target)
}

fn publish(
	app: &AppHandle,
	plan: InventoryBatch,
	placement: BatchBackupPlacement,
	cancellation: &AtomicBool,
) -> ExecutedBatch {
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
		|member, directories| {
			let Ok(mut session) = engine.engine.lock() else {
				return EntryResult::plain(BatchOutcome::Rejected);
			};
			// Recheck after the engine lock, before authorizing any backup output.
			if cancellation.load(Ordering::Acquire) {
				return EntryResult::plain(BatchOutcome::Cancelled);
			}
			if inventory.validate_batch_generation(generation).is_err() {
				return EntryResult::plain(BatchOutcome::InventoryChanged);
			}
			if member.authority.validate().is_err() {
				return EntryResult::plain(BatchOutcome::SourceUnavailable);
			}
			let Ok(catalog) = backup::catalog(&mut session) else {
				return EntryResult::plain(BatchOutcome::Rejected);
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
						Err(_) => return EntryResult::plain(BatchOutcome::Rejected),
					}
				}
			};
			create_one(catalog, member, destination, directories)
		},
	)
}

struct ExecutedBatch {
	report: BatchBackupReport,
	reviews: ReviewRows,
}

struct EntryResult {
	outcome: BatchOutcome,
	backup_name: Option<String>,
	review: Option<ReviewEvidence>,
}

impl EntryResult {
	fn plain(outcome: BatchOutcome) -> Self {
		Self {
			outcome,
			backup_name: None,
			review: None,
		}
	}

	fn needs_review(
		kind: ReviewKind,
		backup: &Path,
		recovery: Option<&Path>,
		reason: &dyn std::fmt::Display,
		directories: &mut SharedDirectories,
	) -> Self {
		Self {
			outcome: BatchOutcome::NeedsReview,
			backup_name: leaf_name(backup),
			review: Some(ReviewEvidence::new(
				kind,
				backup,
				recovery,
				reason,
				directories,
			)),
		}
	}
}

fn execute(
	members: Vec<InventoryBatchMember>,
	mut stop: impl FnMut() -> Option<BatchOutcome>,
	mut create: impl FnMut(&InventoryBatchMember, &mut SharedDirectories) -> EntryResult,
) -> ExecutedBatch {
	let mut report = BatchBackupReport {
		run_id: None,
		entries: Vec::with_capacity(members.len()),
		created: 0,
		failed: 0,
		skipped: 0,
	};
	let mut reviews = ReviewRows::new();
	let mut directories = SharedDirectories::new();
	let mut stopped = None;
	for (index, member) in members.into_iter().enumerate() {
		let EntryResult {
			outcome,
			backup_name,
			review,
		} = match stopped.or_else(&mut stop) {
			Some(reason) => EntryResult::plain(reason),
			None => create(&member, &mut directories),
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
		if let Some(evidence) = review {
			reviews.insert(index, evidence);
		}
		report.entries.push(BatchEntry {
			bank_id: member.id,
			name: member.name,
			outcome,
			backup_name,
		});
	}
	ExecutedBatch { report, reviews }
}

fn create_one(
	catalog: &mut NativeBackupCatalog,
	member: &InventoryBatchMember,
	placement: BackupPlacement<'_>,
	directories: &mut SharedDirectories,
) -> EntryResult {
	let Ok(source) = backup_source(member.authority.path()) else {
		return EntryResult::plain(BatchOutcome::SourceUnavailable);
	};
	match catalog.create(&source, placement) {
		Ok(id) => EntryResult {
			outcome: BatchOutcome::Created,
			backup_name: leaf_name(id.path()),
			review: None,
		},
		Err(NativeBackupError::Policy(error)) => review_policy_error(*error, directories),
		Err(_) => EntryResult::plain(BatchOutcome::Rejected),
	}
}

fn review_policy_error(
	error: BackupError<StorageError, CommitIssue>,
	directories: &mut SharedDirectories,
) -> EntryResult {
	match error {
		BackupError::MissingFile | BackupError::TargetChanged => {
			EntryResult::plain(BatchOutcome::SourceUnavailable)
		}
		BackupError::UncertainBackup { path, issue } => EntryResult::needs_review(
			ReviewKind::SnapshotCommitUncertain,
			&path,
			Some(issue.recovery().path()),
			issue.error(),
			directories,
		),
		BackupError::Metadata { record, failure } => match failure {
			RecordFailure::Rejected(error) => EntryResult::needs_review(
				ReviewKind::MetadataRejected,
				record.id().path(),
				None,
				&error,
				directories,
			),
			RecordFailure::Uncertain(issue) => EntryResult::needs_review(
				ReviewKind::MetadataCommitUncertain,
				record.id().path(),
				Some(issue.recovery().path()),
				issue.error(),
				directories,
			),
		},
		_ => EntryResult::plain(BatchOutcome::Rejected),
	}
}

fn leaf_name(path: &Path) -> Option<String> {
	path.file_name()
		.map(|value| value.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests;
