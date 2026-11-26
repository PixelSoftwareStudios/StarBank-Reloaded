use std::{ffi::OsStr, path::Path};

use serde::{Deserialize, Serialize};
use starbank_editor::{WireReply, max_attachment_bytes};
use starbank_fs::{CommitIntent, CommitOutcome};
use tauri::{AppHandle, Manager as _, State, WebviewWindow};

use crate::commands::{self, CommandError, DesktopEngine};
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
pub(crate) async fn export_bank_copy(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	control: String,
	suggested_name: String,
) -> Result<Option<ExportReply>, CommandError> {
	if !can_export_bank() {
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

fn needs_confirmation(json: &str) -> Result<bool, CommandError> {
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

fn prepare(app: &AppHandle, control: &str) -> Result<WireReply, CommandError> {
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
	let mut engine = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let reply = engine.dispatch(control, None);
	if let Some(bytes) = reply.bytes {
		match starbank_fs::commit(
			path,
			&bytes,
			CommitIntent::CreateNew,
			max_attachment_bytes(),
		) {
			CommitOutcome::CommittedVerified(_) => {}
			CommitOutcome::RejectedUnchanged(error) => {
				return Err(CommandError::WriteRejected(error.to_string()));
			}
			CommitOutcome::CommitUncertain(_) => return Err(CommandError::WriteUncertain),
		}
	}
	Ok(ExportReply {
		name,
		control: reply.json,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

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
