use std::sync::{
	Mutex,
	atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use starbank_editor::MAX_CONTROL_BYTES;
use tauri::{
	Manager as _, State, WebviewWindow,
	ipc::{InvokeBody, Request, Response},
};

use crate::{frame, recovery::RecoveryReceipt, save::DesktopSession, selection};

#[derive(Default)]
pub(crate) struct DesktopEngine {
	pub(crate) engine: Mutex<DesktopSession>,
	dialog_open: AtomicBool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandError {
	DialogBusy,
	InvalidControl,
	InvalidPacket,
	OpenRequiresPicker,
	Unavailable,
	WriteRejected(String),
	ActiveWriteRejected(String),
	ExternalBankChanged,
	ConflictReviewChanged,
	CannotCompareBank(String),
	ActiveWriteUncertain,
	RecoveryRequired(Box<RecoveryReceipt>),
	SavedNeedsReopen,
	File(selection::OpenError),
}

#[derive(Serialize)]
pub(crate) struct OpenedBank {
	pub(crate) name: String,
	pub(crate) control: String,
	pub(crate) can_overwrite: bool,
	pub(crate) breadcrumbs: Vec<String>,
}

#[derive(Deserialize)]
struct Envelope {
	command: Route,
}

#[derive(Deserialize)]
struct Route {
	kind: String,
}

impl DesktopEngine {
	pub(crate) fn dialog(&self) -> Result<DialogGuard<'_>, CommandError> {
		self.dialog_open
			.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
			.map_err(|_| CommandError::DialogBusy)?;
		Ok(DialogGuard(&self.dialog_open))
	}
}

pub(crate) struct DialogGuard<'a>(&'a AtomicBool);

impl Drop for DialogGuard<'_> {
	fn drop(&mut self) {
		self.0.store(false, Ordering::Release);
	}
}

#[tauri::command]
pub(crate) async fn open_bank(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	control: String,
) -> Result<Option<OpenedBank>, CommandError> {
	if !is_request(&control, OPEN_REQUEST)? {
		return Err(CommandError::InvalidControl);
	}
	let _dialog = state.dialog()?;
	let Some(selection) = selection::pick_bank(&window).await else {
		state
			.engine
			.lock()
			.map_err(|_| CommandError::Unavailable)?
			.clear_discard();
		return Ok(None);
	};
	let path = selection.path().to_path_buf();
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		state
			.engine
			.lock()
			.map_err(|_| CommandError::Unavailable)?
			.open_path(&control, &path)
			.map(Some)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn dispatch_bank(
	window: WebviewWindow,
	request: Request<'_>,
) -> Result<Response, CommandError> {
	let InvokeBody::Raw(frame) = request.body() else {
		return Err(CommandError::InvalidPacket);
	};
	let (control, bytes) = frame::decode(frame).ok_or(CommandError::InvalidPacket)?;
	if is_request(control, OPEN_REQUEST)? {
		return Err(CommandError::OpenRequiresPicker);
	}
	let control = control.to_owned();
	let bytes = bytes.map(<[u8]>::to_vec);
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let reply = state
			.engine
			.lock()
			.map_err(|_| CommandError::Unavailable)?
			.dispatch(&control, bytes.as_deref());
		frame::encode(reply)
			.map(Response::new)
			.ok_or(CommandError::InvalidPacket)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

pub(crate) const OPEN_REQUEST: &str = "open";

pub(crate) fn is_request(control: &str, kind: &str) -> Result<bool, CommandError> {
	if control.len() > MAX_CONTROL_BYTES {
		return Err(CommandError::InvalidControl);
	}
	let envelope: Envelope =
		serde_json::from_str(control).map_err(|_| CommandError::InvalidControl)?;
	Ok(envelope.command.kind == kind)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn only_open_uses_the_native_file_picker_route() {
		assert!(matches!(
			is_request(r#"{"command":{"kind":"open"}}"#, OPEN_REQUEST),
			Ok(true)
		));
		assert!(matches!(
			is_request(r#"{"command":{"kind":"replace_raw"}}"#, OPEN_REQUEST),
			Ok(false)
		));
		assert!(matches!(
			is_request(r#"{"command":{"kind":"open","kind":"raw"}}"#, OPEN_REQUEST),
			Err(CommandError::InvalidControl)
		));
	}

	#[test]
	fn native_dialogs_are_exclusive_and_released_when_closed() {
		let state = DesktopEngine::default();
		let open = state.dialog().expect("acquire dialog");
		assert!(matches!(state.dialog(), Err(CommandError::DialogBusy)));
		drop(open);
		assert!(state.dialog().is_ok());
	}
}
