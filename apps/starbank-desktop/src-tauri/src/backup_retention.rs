use std::{
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use serde::Serialize;
use starbank_backup::RetentionPolicy;
use starbank_fs::{
	BackupHealth, BackupPlacement, BackupVerification, BankSource, Fingerprint,
	NativeBackupCatalog, backup_source,
};
use tauri::{AppHandle, Manager as _, WebviewWindow};

use crate::{
	backup::fresh_catalog,
	backup_location::retention_scope,
	commands::DesktopEngine,
	save::{ActiveTarget, trusted_fingerprint},
};

const MAX_REVIEW_RECORDS: usize = 2_048;
const MAX_REVIEW_PAYLOAD_BYTES: usize = 128 * 1_024 * 1_024;
const MAX_OLDER_ROWS: usize = 100;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetentionReview {
	keep_newest: usize,
	verified_count: usize,
	retained_count: usize,
	older_count: usize,
	missing_count: usize,
	unverified_count: usize,
	rejected_metadata: usize,
	older: Vec<OlderBackup>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OlderBackup {
	name: String,
	timestamp_utc: String,
	byte_length: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetentionReviewError {
	KeepAll,
	ReviewTooLarge,
	SameFolder,
	ScopeChanged,
	StaleBank,
	Unavailable,
}

#[derive(Debug, Eq, PartialEq)]
struct BankSelection {
	path: PathBuf,
	fingerprint: Fingerprint,
}

#[tauri::command]
pub(crate) async fn review_backup_retention(
	window: WebviewWindow,
	target: ActiveTarget,
) -> Result<RetentionReview, RetentionReviewError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || check_scope_and_review(&app, &target))
		.await
		.map_err(|_| RetentionReviewError::Unavailable)?
}

fn check_scope_and_review(
	app: &AppHandle,
	target: &ActiveTarget,
) -> Result<RetentionReview, RetentionReviewError> {
	let engine = app.state::<DesktopEngine>();
	let selected = pin_selection(&engine, target)?;
	let scope = retention_scope(app).map_err(|_| RetentionReviewError::Unavailable)?;
	let folder = &scope.0;
	let keep_newest = scope.1.ok_or(RetentionReviewError::KeepAll)?;
	let source = backup_source(&selected.path).map_err(|_| RetentionReviewError::Unavailable)?;
	let mut catalog = fresh_catalog().map_err(|_| RetentionReviewError::Unavailable)?;
	folder.authorize(&mut catalog);
	let review = review_catalog(&mut catalog, &source, folder.path(), keep_newest)?;
	if retention_scope(app).map_err(|_| RetentionReviewError::ScopeChanged)? != scope {
		return Err(RetentionReviewError::ScopeChanged);
	}
	if pin_selection(&engine, target)? != selected {
		return Err(RetentionReviewError::StaleBank);
	}
	Ok(review)
}

fn pin_selection(
	engine: &DesktopEngine,
	target: &ActiveTarget,
) -> Result<BankSelection, RetentionReviewError> {
	let session = engine
		.engine
		.lock()
		.map_err(|_| RetentionReviewError::Unavailable)?;
	let (bank, _) = session
		.selected_for(target)
		.map_err(|_| RetentionReviewError::StaleBank)?;
	let fingerprint = trusted_fingerprint(bank).map_err(|_| RetentionReviewError::StaleBank)?;
	Ok(BankSelection {
		path: bank.path.clone(),
		fingerprint: fingerprint.clone(),
	})
}

fn review_catalog(
	catalog: &mut NativeBackupCatalog,
	source: &BankSource,
	managed: &Path,
	keep_newest: NonZeroUsize,
) -> Result<RetentionReview, RetentionReviewError> {
	if source.path().parent() == Some(managed) {
		return Err(RetentionReviewError::SameFolder);
	}
	let rejected_metadata = catalog
		.load(
			source,
			BackupPlacement::Managed(managed),
			BackupVerification::Recorded,
		)
		.map_err(|_| RetentionReviewError::Unavailable)?;
	let history = catalog.history(source);
	check_budget(
		history
			.iter()
			.map(|(record, _)| record.integrity().byte_length),
	)?;
	let ids = history
		.into_iter()
		.map(|(record, _)| record.id().clone())
		.collect::<Vec<_>>();
	for id in ids {
		// Verification updates health to verified, missing, or unverified.
		let _ = catalog.verify(&id);
	}

	let history = catalog.history(source);
	let mut verified_count = 0;
	let mut missing_count = 0;
	let mut unverified_count = 0;
	for (_, health) in history {
		match health {
			BackupHealth::Verified => verified_count += 1,
			BackupHealth::Missing => missing_count += 1,
			BackupHealth::Unverified => unverified_count += 1,
		}
	}

	let policy = RetentionPolicy::KeepNewest(keep_newest);
	let candidates = catalog.retention_candidates(source, policy);
	let older = candidates
		.iter()
		.take(MAX_OLDER_ROWS)
		.map(|record| OlderBackup {
			name: record
				.id()
				.path()
				.file_name()
				.map(|name| name.to_string_lossy().into_owned())
				.unwrap_or_default(),
			timestamp_utc: record.timestamp().to_string(),
			byte_length: record.integrity().byte_length,
		})
		.collect();
	Ok(RetentionReview {
		keep_newest: keep_newest.get(),
		verified_count,
		retained_count: verified_count.min(keep_newest.get()),
		older_count: candidates.len(),
		missing_count,
		unverified_count,
		rejected_metadata,
		older,
	})
}

fn check_budget(
	mut lengths: impl ExactSizeIterator<Item = usize>,
) -> Result<(), RetentionReviewError> {
	if lengths.len() > MAX_REVIEW_RECORDS
		|| lengths
			.try_fold(0usize, usize::checked_add)
			.is_none_or(|bytes| bytes > MAX_REVIEW_PAYLOAD_BYTES)
	{
		return Err(RetentionReviewError::ReviewTooLarge);
	}
	Ok(())
}

#[cfg(test)]
mod tests;
