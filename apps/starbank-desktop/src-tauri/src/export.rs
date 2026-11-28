use std::{ffi::OsStr, path::Path};

use serde::{Deserialize, Serialize};
use starbank_editor::{WireReply, max_attachment_bytes};
use starbank_fs::{CommitIntent, CommitOutcome};
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::commands::{self, CommandError, DesktopEngine};
use crate::recovery::{RecoveryOperation, WriteContext, uncertain_result};
use crate::save::DesktopSession;
use crate::selection::{BANK_EXTENSION, BANK_FILTER, bank_name};

const PREPARE_REQUEST: &str = "prepare";
const DEFAULT_FILE: &str = "Bank.SC2Bank";

#[derive(Serialize)]
pub(crate) struct ExportReply {
	name: String,
	control: String,
}

#[derive(Deserialize)]
struct Envelope {
	result: Prepared,
}

#[derive(Deserialize)]
struct Prepared {
	payload: PreparedStatus,
}

#[derive(Deserialize)]
struct PreparedStatus {
	signature_status: SignatureStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SignatureStatus {
	Valid,
	Unsigned,
	Invalid,
	Malformed,
	MissingContext,
	Unsupported,
}

#[tauri::command]
pub(crate) fn can_export_bank() -> bool {
	cfg!(any(target_os = "linux", target_os = "macos"))
}

#[tauri::command]
pub(crate) fn can_create_bank_copy() -> bool {
	cfg!(any(target_os = "linux", target_os = "macos", windows))
}

#[tauri::command]
pub(crate) async fn export_bank_copy(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	control: String,
	suggested_name: String,
) -> Result<Option<ExportReply>, CommandError> {
	if !can_create_bank_copy() {
		return Err(CommandError::Unavailable);
	}
	if !commands::is_request(&control, PREPARE_REQUEST)? {
		return Err(CommandError::InvalidControl);
	}

	let _dialog = state.dialog()?;
	let Some(destination) = rfd::AsyncFileDialog::new()
		.set_parent(&window)
		.set_title("Export bank copy")
		.set_file_name(file_name_suggestion(&suggested_name))
		.add_filter(BANK_FILTER, &[BANK_EXTENSION])
		.save_file()
		.await
	else {
		return Ok(None);
	};
	let path = destination.path().to_path_buf();
	let name = bank_name(&path).map_err(CommandError::File)?;
	let app = window.app_handle().clone();
	let preview = {
		let app = app.clone();
		let control = control.clone();
		tauri::async_runtime::spawn_blocking(move || prepare(&app, &control))
			.await
			.map_err(|_| CommandError::Unavailable)??
	};
	if preview.bytes.is_none() {
		return Ok(Some(ExportReply {
			name,
			control: preview.json,
		}));
	}
	let confirmed = if needs_confirmation(&preview.json)? {
		confirm_unverified_signature(&window).await
	} else {
		true
	};
	if !confirmed {
		return Ok(None);
	}
	tauri::async_runtime::spawn_blocking(move || publish(&app, &control, &path, name))
		.await
		.map_err(|_| CommandError::Unavailable)?
		.map(Some)
}

fn file_name_suggestion(name: &str) -> &str {
	if !name.contains(['/', '\\'])
		&& Path::new(name).file_name() == Some(OsStr::new(name))
		&& bank_name(Path::new(name)).is_ok()
	{
		name
	} else {
		DEFAULT_FILE
	}
}

pub(crate) fn needs_confirmation(json: &str) -> Result<bool, CommandError> {
	let envelope: Envelope = serde_json::from_str(json).map_err(|_| CommandError::InvalidPacket)?;
	Ok(!matches!(
		envelope.result.payload.signature_status,
		SignatureStatus::Valid | SignatureStatus::Unsigned
	))
}

async fn confirm_unverified_signature(window: &WebviewWindow) -> bool {
	rfd::AsyncMessageDialog::new()
		.set_parent(window)
		.set_title("Export with an unverified signature?")
		.set_description("StarCraft II may refuse this copy if the signature is invalid.")
		.set_level(rfd::MessageLevel::Warning)
		.set_buttons(rfd::MessageButtons::OkCancel)
		.show()
		.await
		== rfd::MessageDialogResult::Ok
}

pub(crate) fn prepare(app: &AppHandle, control: &str) -> Result<WireReply, CommandError> {
	Ok(app
		.state::<DesktopEngine>()
		.engine
		.lock()
		.map_err(|_| CommandError::Unavailable)?
		.dispatch(control, None))
}

fn publish(
	app: &AppHandle,
	control: &str,
	path: &Path,
	name: String,
) -> Result<ExportReply, CommandError> {
	let state = app.state::<DesktopEngine>();
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	publish_copy(&mut session, control, path, name)
}

fn publish_copy(
	session: &mut DesktopSession,
	control: &str,
	path: &Path,
	name: String,
) -> Result<ExportReply, CommandError> {
	let reply = session.dispatch(control, None);
	if let Some(bytes) = reply.bytes {
		commit_new_copy(path, &bytes, RecoveryOperation::ExportCopy)?;
	}
	Ok(ExportReply {
		name,
		control: reply.json,
	})
}

pub(crate) fn commit_new_copy(
	path: &Path,
	bytes: &[u8],
	operation: RecoveryOperation,
) -> Result<(), CommandError> {
	match starbank_fs::commit(path, bytes, CommitIntent::CreateNew, max_attachment_bytes()) {
		CommitOutcome::CommittedVerified(_) => Ok(()),
		CommitOutcome::RejectedUnchanged(error) => {
			Err(CommandError::WriteRejected(error.to_string()))
		}
		CommitOutcome::CommitUncertain(issue) => Err(uncertain_result(
			WriteContext {
				operation,
				destination: path,
				backup: None,
			},
			Some(issue.recovery().path()),
			issue.error(),
		)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[cfg(windows)]
	#[test]
	fn windows_exports_a_new_copy_without_changing_the_editor_or_source() -> std::io::Result<()> {
		use std::fs;

		use serde_json::{Value, json};
		use starbank_editor::PROTOCOL_VERSION;

		const BANK: &[u8] = b"<Bank><Section name='Original'/></Bank>";
		const EDITED: &[u8] = b"<Bank><Section name='Edited'/></Bank>";
		const OCCUPANT: &[u8] = b"<Bank><Section name='Independent'/></Bank>";
		assert!(can_create_bank_copy());
		assert!(!can_export_bank());

		let folder = tempfile::tempdir()?;
		let root = folder.path().canonicalize()?;
		let source = root.join("Source.SC2Bank");
		let destination = root.join("Exported.SC2Bank");
		let occupied = root.join("Occupied.SC2Bank");
		fs::write(&source, BANK)?;
		fs::write(&occupied, OCCUPANT)?;
		let mut session = DesktopSession::default();
		let open = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "open",
			"command": {"kind": "open"}
		});
		session
			.open_path(&open.to_string(), &source)
			.expect("open source");
		let opening = session
			.editor
			.native_active_target()
			.expect("active source");
		let edit = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "edit",
			"document_id": opening.document_id,
			"expected_revision": opening.revision.to_string(),
			"command": {"kind": "replace_raw"}
		});
		session.dispatch(&edit.to_string(), Some(EDITED));
		let current = session
			.editor
			.native_active_target()
			.expect("active edited bank");
		assert!(current.dirty);
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "export",
			"document_id": current.document_id,
			"expected_revision": current.revision.to_string(),
			"command": {"kind": PREPARE_REQUEST}
		})
		.to_string();
		let preview = session.dispatch(&control, None);
		let expected = preview.bytes.expect("prepare export");
		assert_ne!(expected.as_slice(), BANK);
		assert!(!needs_confirmation(&preview.json).expect("unsigned export"));

		assert!(matches!(
			publish_copy(&mut session, &control, &occupied, "Occupied.SC2Bank".into()),
			Err(CommandError::WriteRejected(_))
		));
		let exported = publish_copy(
			&mut session,
			&control,
			&destination,
			"Exported.SC2Bank".into(),
		)
		.expect("publish verified export");
		assert_eq!(exported.name, "Exported.SC2Bank");
		let result: Value = serde_json::from_str(&exported.control).expect("export receipt");
		assert_eq!(result["result"]["kind"], "prepared");
		assert_eq!(fs::read(&destination)?, expected);
		assert_eq!(fs::read(&source)?, BANK);
		assert_eq!(fs::read(&occupied)?, OCCUPANT);
		assert_eq!(session.editor.native_active_target(), Some(current));
		assert_eq!(
			session.selected.expect("original selection retained").path,
			source
		);
		assert_eq!(fs::read_dir(root)?.count(), 3, "no pending files");
		Ok(())
	}

	#[test]
	fn export_suggestions_accept_only_one_bank_filename() {
		assert_eq!(file_name_suggestion("Local.Sc2Bank"), "Local.Sc2Bank");
		for value in [
			"",
			"notes.txt",
			"/tmp/Bank.sc2bank",
			"../Bank.sc2bank",
			"C:\\Bank.sc2bank",
		] {
			assert_eq!(file_name_suggestion(value), DEFAULT_FILE);
		}
	}

	#[test]
	fn prepared_invalid_or_unverified_signatures_require_confirmation() {
		for value in [
			"valid",
			"unsigned",
			"invalid",
			"malformed",
			"missing_context",
			"unsupported",
		] {
			let json = format!(r#"{{"result":{{"payload":{{"signature_status":"{value}"}}}}}}"#);
			let requires = needs_confirmation(&json).expect("known signature status");
			assert_eq!(requires, !matches!(value, "valid" | "unsigned"));
		}
	}
}
