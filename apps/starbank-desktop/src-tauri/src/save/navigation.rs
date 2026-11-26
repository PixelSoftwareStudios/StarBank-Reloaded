use starbank_core::Revision;
use tauri::{State, WebviewWindow};

use crate::{
	close_guard::{CloseChoice, ask_unsaved},
	commands::{CommandError, DesktopEngine},
};

use super::ActiveTarget;

pub(super) struct DiscardPermit {
	pub(super) request_id: String,
	pub(super) document_id: String,
	pub(super) revision: Revision,
}

#[tauri::command]
pub(crate) async fn confirm_bank_navigation(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
	can_save: bool,
) -> Result<CloseChoice, CommandError> {
	let _dialog = state.dialog()?;
	{
		let session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let _selection = session.selected_for(&target)?;
	}
	let offer_save = can_save && cfg!(any(target_os = "macos", target_os = "linux"));
	let title = if offer_save {
		"Save changes before opening another bank?"
	} else {
		"Discard changes and open another bank?"
	};
	let choice = ask_unsaved(
		&window,
		title,
		"The current bank has unsaved changes.",
		offer_save,
	)
	.await;
	if choice == CloseChoice::Cancel {
		return Ok(choice);
	}
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	let (_, expected) = session.selected_for(&target)?;
	if choice == CloseChoice::Discard {
		session.discard = Some(DiscardPermit {
			request_id: target.request_id,
			document_id: target.document_id,
			revision: expected,
		});
	}
	Ok(choice)
}
