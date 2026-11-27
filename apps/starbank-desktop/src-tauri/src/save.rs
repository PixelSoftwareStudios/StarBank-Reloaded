use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;
use starbank_core::Revision;
use starbank_editor::{
	Engine, NativeSaveReceipt, PROTOCOL_VERSION, WireReply, max_attachment_bytes,
};
use starbank_fs::{
	CommitIntent, CommitOutcome, CommitReceipt, Fingerprint, NativeBackupCatalog, StorageError,
};
use starbank_signature::SignatureStatus;
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::backup;
use crate::commands::{CommandError, DesktopEngine, OpenedBank, is_request};
use crate::conflict::ConflictState;
use crate::selection::{BANK_EXTENSION, BANK_FILTER, bank_name, destination_context};
use crate::settings::{AutomaticBackup, SettingsStore};

pub(crate) mod navigation;
mod open;

use navigation::DiscardPermit;

#[cfg(test)]
mod tests;

const SNAPSHOT: &str = "snapshot";
const CLOSE: &str = "closed";
const MAX_REQUEST_ID_BYTES: usize = 128;

#[derive(Default)]
pub(crate) struct DesktopSession {
	pub(crate) editor: Engine,
	pub(crate) selected: Option<SelectedBank>,
	pub(crate) conflict: ConflictState,
	pub(crate) backups: Option<NativeBackupCatalog>,
	pub(crate) backup_selection: Option<backup::BackupSelection>,
	discard: Option<DiscardPermit>,
}

pub(crate) struct SelectedBank {
	pub(crate) document_id: String,
	pub(crate) path: PathBuf,
	pub(crate) fingerprint: Option<Fingerprint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ActiveTarget {
	pub(crate) request_id: String,
	pub(crate) document_id: String,
	pub(crate) revision: String,
}

impl ActiveTarget {
	pub(crate) fn expected(&self) -> Result<Revision, CommandError> {
		if self.request_id.is_empty()
			|| self.request_id.len() > MAX_REQUEST_ID_BYTES
			|| self.request_id.chars().any(char::is_control)
		{
			return Err(CommandError::InvalidControl);
		}
		self.revision
			.parse()
			.map_err(|_| CommandError::InvalidControl)
	}
}

#[derive(Deserialize)]
struct ReplyControl {
	document_id: Option<String>,
	result: ReplyKind,
}

#[derive(Deserialize)]
struct ReplyKind {
	kind: String,
}

impl DesktopSession {
	pub(crate) fn clear_discard(&mut self) {
		self.discard = None;
	}

	pub(crate) fn dispatch(&mut self, control: &str, bytes: Option<&[u8]>) -> WireReply {
		let reply = self.editor.dispatch(control, bytes);
		if matches!(is_request(control, "close"), Ok(true))
			&& reply_control(&reply).is_ok_and(|parsed| parsed.result.kind == CLOSE)
		{
			self.selected = None;
			self.conflict.invalidate();
			self.backup_selection = None;
			self.backups = None;
			self.discard = None;
		}
		reply
	}

	pub(crate) fn selected_for(
		&self,
		target: &ActiveTarget,
	) -> Result<(&SelectedBank, Revision), CommandError> {
		let expected = target.expected()?;
		let active = self.editor.native_active_target();
		if !active.is_some_and(|active| {
			active.document_id == target.document_id && active.revision == expected
		}) {
			return Err(CommandError::ActiveWriteRejected(
				"The bank changed while this operation was pending.".into(),
			));
		}
		let selected = self
			.selected
			.as_ref()
			.filter(|selected| selected.document_id == target.document_id)
			.ok_or(CommandError::Unavailable)?;
		Ok((selected, expected))
	}

	pub(crate) fn snapshot(
		&mut self,
		request_id: &str,
		document_id: String,
		revision: Revision,
		path: &Path,
	) -> Result<OpenedBank, CommandError> {
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id":request_id,
			"document_id":document_id,
			"expected_revision":revision.to_string(),
			"command":{"kind":SNAPSHOT}
		});
		let reply = self.editor.dispatch(&control.to_string(), None);
		if reply_control(&reply)?.result.kind != SNAPSHOT {
			return Err(CommandError::SavedNeedsReopen);
		}
		Ok(OpenedBank {
			name: bank_name(path).map_err(CommandError::File)?,
			control: reply.json,
			breadcrumbs: crate::selection::display_segments(path),
			can_overwrite: self
				.selected
				.as_ref()
				.is_some_and(|selected| selected.fingerprint.is_some())
				&& starbank_fs::check_replace_scope(path).is_ok(),
		})
	}
}

#[tauri::command]
pub(crate) async fn save_active_bank(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
	with_backup: bool,
) -> Result<Option<OpenedBank>, CommandError> {
	if !cfg!(target_os = "macos") {
		return Err(CommandError::Unavailable);
	}
	let _dialog = state.dialog()?;
	run_save(&window, target, Destination::Overwrite { with_backup }).await
}

#[tauri::command]
pub(crate) async fn save_bank_as(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
) -> Result<Option<OpenedBank>, CommandError> {
	if !cfg!(any(target_os = "macos", target_os = "linux")) {
		return Err(CommandError::Unavailable);
	}
	let _dialog = state.dialog()?;
	let suggested = {
		let session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let (selected, _) = session.selected_for(&target)?;
		bank_name(&selected.path).map_err(CommandError::File)?
	};
	let Some(destination) = rfd::AsyncFileDialog::new()
		.set_parent(&window)
		.set_title("Save Bank As")
		.set_file_name(&suggested)
		.add_filter(BANK_FILTER, &[BANK_EXTENSION])
		.save_file()
		.await
	else {
		return Ok(None);
	};
	let path = destination.path().to_path_buf();
	bank_name(&path).map_err(CommandError::File)?;
	run_save(&window, target, Destination::Create(path)).await
}

enum Destination {
	Overwrite { with_backup: bool },
	Create(PathBuf),
}

struct PreparedSave {
	receipt: NativeSaveReceipt,
	path: PathBuf,
	destination: Destination,
	warn_signature: bool,
	automatic_backup: AutomaticBackup,
}

async fn run_save(
	window: &WebviewWindow,
	target: ActiveTarget,
	destination: Destination,
) -> Result<Option<OpenedBank>, CommandError> {
	let app = window.app_handle().clone();
	let preview_app = app.clone();
	let (target, prepared) =
		tauri::async_runtime::spawn_blocking(move || -> Result<_, CommandError> {
			let state = preview_app.state::<DesktopEngine>();
			let session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
			let prepared = prepare_save(&preview_app, &session, &target, destination)?;
			Ok((target, prepared))
		})
		.await
		.map_err(|_| CommandError::Unavailable)??;

	if prepared.warn_signature
		&& !confirm(
			window,
			"Save with an unverified signature?",
			"The signing context could not be verified. StarCraft II may refuse the saved bank.",
		)
		.await
	{
		return Ok(None);
	}
	if matches!(
		prepared.destination,
		Destination::Overwrite { with_backup: false }
	) && matches!(prepared.automatic_backup, AutomaticBackup::Off)
		&& !confirm(
			window,
			"Overwrite without a backup?",
			"Automatic backups are disabled. The current live bank will be replaced.",
		)
		.await
	{
		return Ok(None);
	}
	tauri::async_runtime::spawn_blocking(move || publish(&app, &target, prepared))
		.await
		.map_err(|_| CommandError::Unavailable)?
		.map(Some)
}

fn prepare_save(
	app: &AppHandle,
	session: &DesktopSession,
	target: &ActiveTarget,
	destination: Destination,
) -> Result<PreparedSave, CommandError> {
	let (selected, expected) = session.selected_for(target)?;
	let path = match &destination {
		Destination::Overwrite { .. } => {
			trusted_fingerprint(selected)?;
			starbank_fs::check_replace_scope(&selected.path).map_err(active_storage_rejected)?;
			selected.path.clone()
		}
		Destination::Create(path) => path.clone(),
	};
	let preferences = SettingsStore::for_app(app)
		.and_then(|settings| settings.save_preferences())
		.map_err(|_| write_rejected("The saved editor preferences could not be read."))?;
	let context = destination_context(&path);
	let signing = context
		.as_ref()
		.filter(|_| preferences.auto_resign_on_save)
		.map_or(
			starbank_editor::ExportSigning::PreserveFor(context.as_ref()),
			starbank_editor::ExportSigning::Sign,
		);
	let receipt = session
		.editor
		.prepare_native_active_save(&target.document_id, expected, signing)
		.map_err(write_rejected)?;
	let warn_signature = if preferences.auto_resign_on_save {
		receipt.signature_status() != SignatureStatus::Valid || context.is_none()
	} else {
		!matches!(
			receipt.signature_status(),
			SignatureStatus::Unsigned | SignatureStatus::Valid
		)
	};
	Ok(PreparedSave {
		receipt,
		path,
		destination,
		warn_signature,
		automatic_backup: preferences.automatic_backup,
	})
}

fn publish(
	app: &AppHandle,
	target: &ActiveTarget,
	prepared: PreparedSave,
) -> Result<OpenedBank, CommandError> {
	let state = app.state::<DesktopEngine>();
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let (selected, _) = session.selected_for(target)?;
	session
		.editor
		.validate_native_active_save(&prepared.receipt)
		.map_err(write_rejected)?;
	let overwrite = matches!(prepared.destination, Destination::Overwrite { .. });
	let commit = match prepared.destination {
		Destination::Overwrite { with_backup } => {
			let expected = trusted_fingerprint(selected)?.clone();
			if selected.path != prepared.path {
				return Err(CommandError::Unavailable);
			}
			backup::overwrite(
				app,
				&mut session,
				&prepared.path,
				&expected,
				prepared.receipt.bytes(),
				with_backup,
				prepared.automatic_backup,
			)?
		}
		Destination::Create(_) => starbank_fs::commit(
			&prepared.path,
			prepared.receipt.bytes(),
			CommitIntent::CreateNew,
			max_attachment_bytes(),
		),
	};
	let committed = verify_write(&mut session, commit, overwrite)?;
	session.conflict.invalidate();
	let id = target.document_id.clone();
	let revision = session
		.editor
		.acknowledge_verified_native_active_save(prepared.receipt)
		.map_err(|_| {
			session.selected = None;
			CommandError::SavedNeedsReopen
		})?;
	session.backup_selection = None;
	if !overwrite {
		session.backups = None;
	}
	session.selected = Some(SelectedBank {
		document_id: id.clone(),
		path: prepared.path.clone(),
		fingerprint: Some(committed.fingerprint().clone()),
	});
	session.snapshot(&target.request_id, id, revision, &prepared.path)
}

pub(crate) fn trusted_fingerprint(selected: &SelectedBank) -> Result<&Fingerprint, CommandError> {
	selected
		.fingerprint
		.as_ref()
		.ok_or(CommandError::ActiveWriteUncertain)
}

pub(crate) fn verify_write(
	session: &mut DesktopSession,
	outcome: CommitOutcome,
	affects_active: bool,
) -> Result<CommitReceipt, CommandError> {
	match outcome {
		CommitOutcome::CommittedVerified(receipt) => Ok(receipt),
		CommitOutcome::RejectedUnchanged(error) if affects_active => {
			Err(active_storage_rejected(error))
		}
		CommitOutcome::RejectedUnchanged(error) => Err(write_rejected(error)),
		CommitOutcome::CommitUncertain(_) => {
			if affects_active && let Some(selected) = &mut session.selected {
				selected.fingerprint = None;
			}
			Err(if affects_active {
				CommandError::ActiveWriteUncertain
			} else {
				CommandError::NewFileUncertain
			})
		}
	}
}

fn active_storage_rejected(error: StorageError) -> CommandError {
	if matches!(error, StorageError::Conflict | StorageError::UnsafePath(_))
		|| error.kind() == std::io::ErrorKind::NotFound
	{
		CommandError::ExternalBankChanged
	} else {
		write_rejected(error)
	}
}

pub(crate) fn write_rejected(error: impl std::fmt::Display) -> CommandError {
	CommandError::ActiveWriteRejected(error.to_string())
}

pub(crate) async fn confirm(window: &WebviewWindow, title: &str, description: &str) -> bool {
	rfd::AsyncMessageDialog::new()
		.set_parent(window)
		.set_title(title)
		.set_description(description)
		.set_level(rfd::MessageLevel::Warning)
		.set_buttons(rfd::MessageButtons::OkCancel)
		.show()
		.await
		== rfd::MessageDialogResult::Ok
}

fn reply_control(reply: &WireReply) -> Result<ReplyControl, CommandError> {
	serde_json::from_str(&reply.json).map_err(|_| CommandError::InvalidPacket)
}
