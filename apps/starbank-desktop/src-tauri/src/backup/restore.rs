use std::path::PathBuf;

use sc2bank::{Limits, parse};
use serde::Serialize;
use starbank_editor::{NativeRestorePolicy, NativeRestoreReceipt, max_attachment_bytes};
use starbank_fs::{BackupPlacement, PreparedRestore, StorageError, read_snapshot};
use starbank_signature::SignatureStatus;
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use super::{backup_selection, catalog, fresh_catalog};
use crate::recovery::{RecoveryOperation, WriteContext, backup_failure, verified_result};
use crate::{
	backup_location::backup_folder,
	commands::{CommandError, DesktopEngine, OpenedBank},
	save::{
		ActiveTarget, DesktopSession, SelectedBank, confirm, trusted_fingerprint, verify_write,
		write_rejected,
	},
	xml_comparison::XmlComparison,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupRestoreReview {
	description: String,
	comparison: XmlComparison,
}

#[tauri::command]
pub(crate) async fn review_active_backup(
	window: WebviewWindow,
	target: ActiveTarget,
	backup_id: usize,
) -> Result<BackupRestoreReview, CommandError> {
	if !cfg!(target_os = "macos") {
		return Err(CommandError::Unavailable);
	}
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		review_backup(&mut session, &target, backup_id)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
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
			let state = prepare_app.state::<DesktopEngine>();
			let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
			let prepared = prepare_reviewed_restore(&mut session, &target, backup_id)?;
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

fn review_backup(
	session: &mut DesktopSession,
	target: &ActiveTarget,
	backup_id: usize,
) -> Result<BackupRestoreReview, CommandError> {
	let prepared = prepare_restore(session, target, backup_id)?;
	let current =
		parse(prepared.storage.current().bytes(), Limits::default()).map_err(write_rejected)?;
	let backup =
		parse(prepared.storage.selected().bytes(), Limits::default()).map_err(write_rejected)?;
	let comparison = XmlComparison::between(current.xml_text(), backup.xml_text());
	session
		.backup_selection
		.as_mut()
		.ok_or(CommandError::InvalidControl)?
		.reviewed_restore = Some(backup_id);
	Ok(BackupRestoreReview {
		description: prepared.description,
		comparison,
	})
}

fn prepare_reviewed_restore(
	session: &mut DesktopSession,
	target: &ActiveTarget,
	backup_id: usize,
) -> Result<Restore, CommandError> {
	let (_, revision) = session.selected_for(target)?;
	let selection = backup_selection(session, target, revision, backup_id)?;
	if selection.reviewed_restore != Some(backup_id) {
		return Err(CommandError::InvalidControl);
	}
	let prepared = prepare_restore(session, target, backup_id)?;
	session.backup_selection = None;
	Ok(prepared)
}

fn prepare_restore(
	session: &DesktopSession,
	target: &ActiveTarget,
	backup_id: usize,
) -> Result<Restore, CommandError> {
	let (selected, revision) = session.selected_for(target)?;
	let backup = &backup_selection(session, target, revision, backup_id)?.records[backup_id];
	if selected.path != backup.record.source().path() {
		return Err(CommandError::InvalidControl);
	}
	starbank_fs::check_replace_scope(&selected.path).map_err(write_rejected)?;
	let current = read_snapshot(&selected.path, max_attachment_bytes()).map_err(write_rejected)?;
	if current.fingerprint() != trusted_fingerprint(selected)? {
		return Err(write_rejected(StorageError::Conflict));
	}
	let path = selected.path.clone();
	let mut catalog = fresh_catalog()?;
	let placement = backup.placement(&mut catalog);
	let storage = catalog
		.prepare_restore(
			backup.record.source(),
			current,
			backup.record.id().path(),
			placement,
		)
		.map_err(write_rejected)?;
	if storage.record() != &backup.record {
		return Err(write_rejected(
			"This backup changed. Open the backup list again.",
		));
	}
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

#[cfg(all(test, target_os = "macos"))]
mod tests;

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
	let catalog = catalog(&mut session)?;
	managed.authorize(catalog);
	let effect = catalog
		.restore(prepared.storage, BackupPlacement::Managed(managed.path()))
		.map_err(|failure| {
			backup_failure(
				WriteContext {
					operation: RecoveryOperation::Restore,
					destination: &prepared.path,
					backup: None,
				},
				failure,
			)
		})?;
	let context = WriteContext {
		operation: RecoveryOperation::Restore,
		destination: &prepared.path,
		backup: effect.backup.as_ref(),
	};
	let committed = verify_write(&mut session, effect.commit, context)?;
	let acknowledged = session
		.editor
		.acknowledge_verified_native_active_restore(prepared.editor);
	let outcome = verified_result(&mut session, context, &committed, acknowledged)?;

	session.backups = None;
	session.selected = Some(SelectedBank {
		document_id: outcome.document_id.clone(),
		path: prepared.path.clone(),
		fingerprint: Some(committed.fingerprint().clone()),
	});
	let snapshot = session.snapshot(
		&target.request_id,
		outcome.document_id,
		outcome.revision,
		&prepared.path,
	);
	verified_result(&mut session, context, &committed, snapshot)
}
