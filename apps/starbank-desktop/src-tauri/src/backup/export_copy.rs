use std::path::Path;

use sc2bank::{Limits, parse};
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use super::{SelectedBackup, fresh_catalog, take_backup};
use crate::recovery::RecoveryOperation;
use crate::{
	commands::{CommandError, DesktopEngine},
	export::{can_export_bank, commit_new_copy},
	save::{ActiveTarget, trusted_fingerprint},
	selection::{BANK_EXTENSION, BANK_FILTER, bank_name},
};

#[tauri::command]
pub(crate) async fn export_active_backup(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
	backup_id: usize,
) -> Result<Option<String>, CommandError> {
	if !can_export_bank() {
		return Err(CommandError::Unavailable);
	}
	let _dialog = state.dialog()?;
	let (suggested, backup) = {
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let (selected, revision) = session.selected_for(&target)?;
		trusted_fingerprint(selected)?;
		let suggested = bank_name(&selected.path).map_err(CommandError::File)?;
		(
			suggested,
			take_backup(&mut session, &target, revision, backup_id)?,
		)
	};
	let Some(destination) = rfd::AsyncFileDialog::new()
		.set_parent(&window)
		.set_title("Save Bank Backup Copy")
		.set_file_name(&suggested)
		.add_filter(BANK_FILTER, &[BANK_EXTENSION])
		.save_file()
		.await
	else {
		return Ok(None);
	};
	let destination = destination.path().to_path_buf();
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || publish(&app, &target, &backup, &destination))
		.await
		.map_err(|_| CommandError::Unavailable)?
		.map(Some)
}

fn publish(
	app: &AppHandle,
	target: &ActiveTarget,
	backup: &SelectedBackup,
	destination: &Path,
) -> Result<String, CommandError> {
	let state = app.state::<DesktopEngine>();
	let session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let (selected, _) = session.selected_for(target)?;
	trusted_fingerprint(selected)?;
	if selected.path != backup.record.source().path() {
		return Err(CommandError::InvalidControl);
	}
	save_copy(backup, destination)
}

fn save_copy(backup: &SelectedBackup, destination: &Path) -> Result<String, CommandError> {
	let name = bank_name(destination).map_err(CommandError::File)?;
	let mut catalog = fresh_catalog()?;
	let placement = backup.placement(&mut catalog);
	let snapshot = catalog
		.read_verified_backup(&backup.record, placement)
		.map_err(|error| CommandError::WriteRejected(error.to_string()))?;
	parse(snapshot.bytes(), Limits::default())
		.map_err(|error| CommandError::WriteRejected(error.to_string()))?;
	commit_new_copy(destination, snapshot.bytes(), RecoveryOperation::BackupCopy)?;
	Ok(name)
}

#[cfg(test)]
mod tests;
