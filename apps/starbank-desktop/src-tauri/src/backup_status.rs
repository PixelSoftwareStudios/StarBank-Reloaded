use std::path::{Path, PathBuf};

use serde::Serialize;
use starbank_fs::{BackupPlacement, BackupVerification, Fingerprint, backup_source};
use tauri::{AppHandle, Manager as _, WebviewWindow};

use crate::{
	backup::fresh_catalog,
	backup_location::{ManagedBackupHistory, backup_history},
	commands::{CommandError, DesktopEngine},
	save::{ActiveTarget, trusted_fingerprint},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiveBackupStatus {
	state: Status,
	record_count: usize,
	rejected_metadata: usize,
	latest_timestamp_utc: Option<String>,
	incomplete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
	NeverBackedUp,
	Recorded,
	Unknown,
}

#[derive(Debug, Eq, PartialEq)]
struct PinnedSelection {
	path: PathBuf,
	fingerprint: Fingerprint,
}

#[tauri::command]
pub(crate) async fn active_backup_status(
	window: WebviewWindow,
	target: ActiveTarget,
) -> Result<ActiveBackupStatus, CommandError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || status_for_selection(&app, &target))
		.await
		.map_err(|_| CommandError::Unavailable)?
}

fn status_for_selection(
	app: &AppHandle,
	target: &ActiveTarget,
) -> Result<ActiveBackupStatus, CommandError> {
	let engine = app.state::<DesktopEngine>();
	let pinned = selected(&engine, target)?;
	let status = recorded_status(&pinned.path, &backup_history(app))?;
	if selected(&engine, target)? != pinned {
		return Err(CommandError::Unavailable);
	}
	Ok(status)
}

fn selected(
	engine: &DesktopEngine,
	target: &ActiveTarget,
) -> Result<PinnedSelection, CommandError> {
	let session = engine
		.engine
		.lock()
		.map_err(|_| CommandError::Unavailable)?;
	let (bank, _) = session.selected_for(target)?;
	Ok(PinnedSelection {
		path: bank.path.clone(),
		fingerprint: trusted_fingerprint(bank)?.clone(),
	})
}

fn recorded_status(
	path: &Path,
	managed: &ManagedBackupHistory,
) -> Result<ActiveBackupStatus, CommandError> {
	let Ok(source) = backup_source(path) else {
		return Ok(ActiveBackupStatus {
			state: Status::Unknown,
			record_count: 0,
			rejected_metadata: 0,
			latest_timestamp_utc: None,
			incomplete: true,
		});
	};
	let mut catalog = fresh_catalog()?;
	let adjacent = catalog.load(
		&source,
		BackupPlacement::Adjacent,
		BackupVerification::Recorded,
	);
	let mut incomplete = managed.unavailable > 0 || adjacent.is_err();
	let mut rejected_metadata = adjacent.unwrap_or_default();
	for folder in &managed.folders {
		if path.parent() == Some(folder.path()) {
			continue;
		}
		folder.authorize(&mut catalog);
		match catalog.load(
			&source,
			BackupPlacement::Managed(folder.path()),
			BackupVerification::Recorded,
		) {
			Ok(rejected) => rejected_metadata += rejected,
			Err(_) => incomplete = true,
		}
	}
	incomplete |= rejected_metadata != 0;
	let history = catalog.history(&source);
	let record_count = history.len();
	let latest_timestamp_utc = history
		.first()
		.map(|(record, _)| record.timestamp().to_string());
	let state = match (record_count, incomplete) {
		(0, true) => Status::Unknown,
		(0, false) => Status::NeverBackedUp,
		_ => Status::Recorded,
	};
	Ok(ActiveBackupStatus {
		state,
		record_count,
		rejected_metadata,
		latest_timestamp_utc,
		incomplete,
	})
}

#[cfg(test)]
mod tests;
