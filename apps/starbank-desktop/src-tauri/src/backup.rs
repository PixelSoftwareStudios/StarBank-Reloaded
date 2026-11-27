use std::num::NonZeroUsize;
use std::path::Path;

use serde::Serialize;
use starbank_core::Revision;
use starbank_editor::max_attachment_bytes;
use starbank_fs::{
	BackupHealth, BackupPlacement, BackupRecord, BackupVerification, Fingerprint,
	NativeBackupCatalog, NativeBackupEffect, NativeBackupPolicy, backup_source,
};
use tauri::{AppHandle, Manager as _, WebviewWindow};

use crate::recovery::{RecoveryOperation, WriteContext, backup_failure};
use crate::save::{ActiveTarget, DesktopSession, trusted_fingerprint, write_rejected};
use crate::settings::AutomaticBackup;
use crate::{
	backup_location::{ManagedBackupFolder, backup_folder, backup_history},
	commands::{CommandError, DesktopEngine},
	export::can_export_bank,
};

pub(crate) mod export_copy;
pub(crate) mod restore;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupChoices {
	records: Vec<BackupChoice>,
	rejected_metadata: usize,
	unavailable_locations: usize,
}

#[derive(Serialize)]
struct BackupChoice {
	id: usize,
	name: String,
	timestamp: String,
	placement: BackupPlacementLabel,
	health: Health,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum BackupPlacementLabel {
	Adjacent,
	Managed,
}

enum Location {
	Adjacent,
	Managed(ManagedBackupFolder),
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Health {
	Verified,
	Missing,
	Unverified,
}

pub(crate) struct BackupSelection {
	request_id: String,
	document_id: String,
	revision: Revision,
	records: Vec<SelectedBackup>,
	reviewed_restore: Option<usize>,
}

struct SelectedBackup {
	record: BackupRecord,
	location: Location,
}

impl SelectedBackup {
	fn placement(&self, catalog: &mut NativeBackupCatalog) -> BackupPlacement<'_> {
		match &self.location {
			Location::Adjacent => {
				catalog.set_managed_directory(None);
				BackupPlacement::Adjacent
			}
			Location::Managed(folder) => {
				folder.authorize(catalog);
				BackupPlacement::Managed(folder.path())
			}
		}
	}
}

fn backup_selection<'a>(
	session: &'a DesktopSession,
	target: &ActiveTarget,
	revision: Revision,
	backup_id: usize,
) -> Result<&'a BackupSelection, CommandError> {
	session
		.backup_selection
		.as_ref()
		.filter(|selection| {
			selection.request_id == target.request_id
				&& selection.document_id == target.document_id
				&& selection.revision == revision
				&& backup_id < selection.records.len()
		})
		.ok_or(CommandError::InvalidControl)
}

fn take_backup(
	session: &mut DesktopSession,
	target: &ActiveTarget,
	revision: Revision,
	backup_id: usize,
) -> Result<SelectedBackup, CommandError> {
	backup_selection(session, target, revision, backup_id)?;
	session
		.backup_selection
		.take()
		.and_then(|selection| selection.records.into_iter().nth(backup_id))
		.ok_or(CommandError::InvalidControl)
}

pub(crate) fn overwrite(
	app: &AppHandle,
	session: &mut DesktopSession,
	path: &Path,
	expected: &Fingerprint,
	bytes: &[u8],
	with_backup: bool,
	automatic: AutomaticBackup,
) -> Result<NativeBackupEffect, CommandError> {
	let source = backup_source(path).map_err(write_rejected)?;
	let catalog = catalog(session)?;
	catalog.set_managed_directory(None);
	let managed;
	let policy = match (with_backup, automatic) {
		(true, _) => NativeBackupPolicy::Explicit,
		(false, AutomaticBackup::Off) => NativeBackupPolicy::Disabled,
		(false, mode) => {
			managed = backup_folder(app, true)?;
			managed.authorize(catalog);
			let placement = BackupPlacement::Managed(managed.path());
			match mode {
				AutomaticBackup::FirstOverwrite => NativeBackupPolicy::FirstInSession(placement),
				AutomaticBackup::EveryOverwrite => NativeBackupPolicy::EveryOverwrite(placement),
				AutomaticBackup::Off => NativeBackupPolicy::Disabled,
			}
		}
	};
	catalog
		.overwrite(&source, expected, bytes, policy)
		.map_err(|failure| {
			backup_failure(
				WriteContext {
					operation: RecoveryOperation::SaveOverwrite,
					destination: path,
					backup: None,
				},
				failure,
			)
		})
}

#[tauri::command]
pub(crate) async fn list_active_backups(
	window: WebviewWindow,
	target: ActiveTarget,
) -> Result<BackupChoices, CommandError> {
	if !can_export_bank() {
		return Err(CommandError::Unavailable);
	}
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || list_backups(&app, &target))
		.await
		.map_err(|_| CommandError::Unavailable)?
}

fn list_backups(app: &AppHandle, target: &ActiveTarget) -> Result<BackupChoices, CommandError> {
	let state = app.state::<DesktopEngine>();
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let (selected, revision) = session.selected_for(target)?;
	trusted_fingerprint(selected)?;
	let source = backup_source(&selected.path).map_err(write_rejected)?;
	let mut history = backup_history(app);
	let mut catalog = fresh_catalog()?;
	let mut rejected_metadata = catalog
		.load(
			&source,
			BackupPlacement::Adjacent,
			BackupVerification::Verify,
		)
		.map_err(write_rejected)?;
	for folder in &history.folders {
		if selected.path.parent() == Some(folder.path()) {
			continue;
		}
		folder.authorize(&mut catalog);
		match catalog.load(
			&source,
			BackupPlacement::Managed(folder.path()),
			BackupVerification::Verify,
		) {
			Ok(rejected) => rejected_metadata += rejected,
			Err(_) => history.unavailable += 1,
		}
	}
	let mut records = Vec::new();
	let mut selections = Vec::new();
	for (record, health) in catalog.history(&source) {
		let id = record.id();
		let folder = history
			.folders
			.iter()
			.find(|folder| id.path().parent() == Some(folder.path()))
			.filter(|folder| selected.path.parent() != Some(folder.path()));
		let (label, location) = match folder {
			Some(folder) => (
				BackupPlacementLabel::Managed,
				Location::Managed(folder.clone()),
			),
			None => (BackupPlacementLabel::Adjacent, Location::Adjacent),
		};
		records.push(BackupChoice {
			id: selections.len(),
			name: id
				.path()
				.file_name()
				.ok_or(CommandError::InvalidPacket)?
				.to_string_lossy()
				.into_owned(),
			timestamp: record.timestamp().to_string(),
			placement: label,
			health: match health {
				BackupHealth::Verified => Health::Verified,
				BackupHealth::Missing => Health::Missing,
				BackupHealth::Unverified => Health::Unverified,
			},
		});
		selections.push(SelectedBackup {
			record: record.clone(),
			location,
		});
	}
	session.backup_selection = Some(BackupSelection {
		request_id: target.request_id.clone(),
		document_id: target.document_id.clone(),
		revision,
		records: selections,
		reviewed_restore: None,
	});
	Ok(BackupChoices {
		records,
		rejected_metadata,
		unavailable_locations: history.unavailable,
	})
}

pub(crate) fn fresh_catalog() -> Result<NativeBackupCatalog, CommandError> {
	Ok(NativeBackupCatalog::new(catalog_limit()?))
}

fn catalog_limit() -> Result<NonZeroUsize, CommandError> {
	NonZeroUsize::new(max_attachment_bytes()).ok_or(CommandError::Unavailable)
}

pub(crate) fn catalog(
	session: &mut DesktopSession,
) -> Result<&mut NativeBackupCatalog, CommandError> {
	let limit = catalog_limit()?;
	Ok(session
		.backups
		.get_or_insert_with(|| NativeBackupCatalog::new(limit)))
}
