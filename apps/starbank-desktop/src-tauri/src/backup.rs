use std::fs;
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use serde::Serialize;
use starbank_core::Revision;
use starbank_editor::{NativeRestorePolicy, NativeRestoreReceipt, max_attachment_bytes};
use starbank_fs::{
	BackupHealth, BackupId, BackupPlacement, BackupVerification, CommitOutcome, Fingerprint,
	NativeBackupCatalog, NativeBackupPolicy, PreparedRestore, backup_source, read_snapshot,
};
use starbank_signature::SignatureStatus;
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::commands::{CommandError, DesktopEngine, OpenedBank};
use crate::save::{
	ActiveTarget, DesktopSession, SelectedBank, confirm, trusted_fingerprint, verify_write,
	write_rejected,
};
use crate::settings::AutomaticBackup;

const BACKUP_DIRECTORY: &str = "Backups";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupChoices {
	records: Vec<BackupChoice>,
	rejected_metadata: usize,
}

#[derive(Serialize)]
struct BackupChoice {
	id: usize,
	name: String,
	timestamp: String,
	placement: Location,
	health: Health,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Location {
	Adjacent,
	Managed,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Health {
	Verified,
	Missing,
	Unverified,
}

pub(crate) struct BackupSelection {
	document_id: String,
	revision: Revision,
	records: Vec<(BackupId, Location)>,
}

pub(crate) fn overwrite(
	app: &AppHandle,
	session: &mut DesktopSession,
	path: &Path,
	expected: &Fingerprint,
	bytes: &[u8],
	with_backup: bool,
	automatic: AutomaticBackup,
) -> Result<CommitOutcome, CommandError> {
	let source = backup_source(path).map_err(write_rejected)?;
	let managed;
	let policy = match (with_backup, automatic) {
		(true, _) => NativeBackupPolicy::Explicit,
		(false, AutomaticBackup::Off) => NativeBackupPolicy::Disabled,
		(false, mode) => {
			managed = backup_folder(app, true)?;
			let placement = BackupPlacement::Managed(&managed);
			match mode {
				AutomaticBackup::FirstOverwrite => NativeBackupPolicy::FirstInSession(placement),
				AutomaticBackup::EveryOverwrite => NativeBackupPolicy::EveryOverwrite(placement),
				AutomaticBackup::Off => NativeBackupPolicy::Disabled,
			}
		}
	};
	Ok(catalog(session)?
		.overwrite(&source, expected, bytes, policy)
		.map_err(write_rejected)?
		.commit)
}

#[tauri::command]
pub(crate) async fn list_active_backups(
	window: WebviewWindow,
	target: ActiveTarget,
) -> Result<BackupChoices, CommandError> {
	if !cfg!(target_os = "macos") {
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
	starbank_fs::check_replace_scope(&selected.path).map_err(write_rejected)?;
	let source = backup_source(&selected.path).map_err(write_rejected)?;
	let managed = backup_folder(app, false)?;
	let mut catalog = fresh_catalog()?;
	let mut rejected_metadata = 0;
	for placement in [
		BackupPlacement::Adjacent,
		BackupPlacement::Managed(&managed),
	] {
		let rejected = catalog
			.load(&source, placement, BackupVerification::Verify)
			.map_err(write_rejected)?;
		rejected_metadata += rejected;
	}
	let mut records = Vec::new();
	let mut selections = Vec::new();
	for (record, health) in catalog.history(&source) {
		let id = record.id();
		let location = if id.path().parent() == Some(&managed) {
			Location::Managed
		} else {
			Location::Adjacent
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
			placement: location,
			health: match health {
				BackupHealth::Verified => Health::Verified,
				BackupHealth::Missing => Health::Missing,
				BackupHealth::Unverified => Health::Unverified,
			},
		});
		selections.push((id.clone(), location));
	}
	session.backup_selection = Some(BackupSelection {
		document_id: target.document_id.clone(),
		revision,
		records: selections,
	});
	Ok(BackupChoices {
		records,
		rejected_metadata,
	})
}

#[tauri::command]
pub(crate) async fn restore_active_bank(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
	backup_id: usize,
) -> Result<Option<OpenedBank>, CommandError> {
	if !cfg!(target_os = "macos") {
		return Err(CommandError::Unavailable);
	}
	let _dialog = state.dialog()?;
	let app = window.app_handle().clone();
	let prepare_app = app.clone();
	let (target, prepared) =
		tauri::async_runtime::spawn_blocking(move || -> Result<_, CommandError> {
			let prepared = prepare_restore(&prepare_app, &target, backup_id)?;
			Ok((target, prepared))
		})
		.await
		.map_err(|_| CommandError::Unavailable)??;
	if !confirm(&window, "Restore this bank backup?", &prepared.description).await {
		return Ok(None);
	}
	tauri::async_runtime::spawn_blocking(move || publish_restore(&app, &target, prepared))
		.await
		.map_err(|_| CommandError::Unavailable)?
		.map(Some)
}

struct Restore {
	path: PathBuf,
	storage: PreparedRestore,
	editor: NativeRestoreReceipt,
	description: String,
}

fn prepare_restore(
	app: &AppHandle,
	target: &ActiveTarget,
	backup_id: usize,
) -> Result<Restore, CommandError> {
	let state = app.state::<DesktopEngine>();
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let (selected, revision) = session.selected_for(target)?;
	starbank_fs::check_replace_scope(&selected.path).map_err(write_rejected)?;
	let current = read_snapshot(&selected.path, max_attachment_bytes()).map_err(write_rejected)?;
	if current.fingerprint() != trusted_fingerprint(selected)? {
		return Err(write_rejected(starbank_fs::StorageError::Conflict));
	}
	let path = selected.path.clone();
	let source = backup_source(&path).map_err(write_rejected)?;
	let (backup, location) = session
		.backup_selection
		.take()
		.filter(|selection| {
			selection.document_id == target.document_id && selection.revision == revision
		})
		.and_then(|selection| selection.records.into_iter().nth(backup_id))
		.ok_or(CommandError::InvalidControl)?;
	let managed = backup_folder(app, false)?;
	let placement = match location {
		Location::Adjacent => BackupPlacement::Adjacent,
		Location::Managed => BackupPlacement::Managed(&managed),
	};
	let storage = catalog(&mut session)?
		.prepare_restore(&source, current, backup.path(), placement)
		.map_err(write_rejected)?;
	let dirty = session
		.editor
		.native_active_target()
		.ok_or(CommandError::Unavailable)?
		.dirty;
	let policy = if dirty {
		NativeRestorePolicy::DiscardChanges
	} else {
		NativeRestorePolicy::RequireClean
	};
	let editor = session
		.editor
		.prepare_native_active_restore(
			&target.document_id,
			revision,
			storage.selected().bytes(),
			policy,
		)
		.map_err(write_rejected)?;
	let description = describe_restore(&storage, editor.signature_status(), dirty);
	Ok(Restore {
		path,
		storage,
		editor,
		description,
	})
}

fn describe_restore(storage: &PreparedRestore, signature: SignatureStatus, dirty: bool) -> String {
	let original = storage.current().bytes();
	let backup = storage.selected().bytes();
	let change = if original == backup {
		String::from("The backup and the current file have identical bytes.")
	} else {
		let first_changed_byte = original
			.iter()
			.zip(backup)
			.take_while(|(live_byte, backup_byte)| live_byte == backup_byte)
			.count()
			+ 1;
		format!(
			"The first difference is at byte {first_changed_byte}. The file changes from {} to {} bytes.",
			original.len(),
			backup.len()
		)
	};
	let timestamp = storage.record().timestamp();
	let mut description = format!(
		"Backup {timestamp} UTC. {change} A reversal backup will be verified before replacing the current file."
	);
	if !matches!(
		signature,
		SignatureStatus::Valid | SignatureStatus::Unsigned
	) {
		description.push_str(" The restored signature could not be verified.");
	}
	if dirty {
		description.push_str(" This also discards the unsaved changes in the editor.");
	}
	description
}

fn publish_restore(
	app: &AppHandle,
	target: &ActiveTarget,
	prepared: Restore,
) -> Result<OpenedBank, CommandError> {
	let state = app.state::<DesktopEngine>();
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let (selected, _) = session.selected_for(target)?;
	if selected.path != prepared.path
		|| trusted_fingerprint(selected)? != prepared.storage.current().fingerprint()
	{
		return Err(CommandError::Unavailable);
	}
	session
		.editor
		.validate_native_active_restore(&prepared.editor)
		.map_err(write_rejected)?;
	let managed = backup_folder(app, true)?;
	let effect = catalog(&mut session)?
		.restore(prepared.storage, BackupPlacement::Managed(&managed))
		.map_err(write_rejected)?;
	let committed = verify_write(&mut session, effect.commit, true)?;
	let outcome = session
		.editor
		.acknowledge_verified_native_active_restore(prepared.editor)
		.map_err(|_| {
			session.selected = None;
			CommandError::SavedNeedsReopen
		})?;
	session.backups = None;
	session.selected = Some(SelectedBank {
		document_id: outcome.document_id.clone(),
		path: prepared.path.clone(),
		fingerprint: Some(committed.fingerprint().clone()),
	});
	session.snapshot(
		&target.request_id,
		outcome.document_id,
		outcome.revision,
		&prepared.path,
	)
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

pub(crate) fn backup_folder(app: &AppHandle, create: bool) -> Result<PathBuf, CommandError> {
	let data = app.path().app_data_dir().map_err(write_rejected)?;
	let folder = data.join(BACKUP_DIRECTORY);
	if create {
		ensure_folder(&data)?;
		ensure_folder(&folder)?;
	}
	Ok(folder)
}

fn ensure_folder(path: &Path) -> Result<(), CommandError> {
	#[cfg(unix)]
	let mut builder = fs::DirBuilder::new();
	#[cfg(not(unix))]
	let builder = fs::DirBuilder::new();
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt as _;
		builder.mode(0o700);
	}
	match builder.create(path) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
			if fs::symlink_metadata(path).map_err(write_rejected)?.is_dir() {
				Ok(())
			} else {
				Err(write_rejected("Backup location is not a local directory."))
			}
		}
		Err(error) => Err(write_rejected(error)),
	}
}
