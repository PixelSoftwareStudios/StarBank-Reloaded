use starbank_core::Revision;
use tauri::{State, WebviewWindow};

use crate::{
	close_guard::{CloseChoice, ask_unsaved},
	commands::{CommandError, DesktopEngine},
	export::can_create_bank_copy,
};

use super::{ActiveTarget, DesktopSession};

pub(super) struct DiscardPermit {
	pub(super) request_id: String,
	pub(super) document_id: String,
	pub(super) revision: Revision,
}

struct NavigationState {
	dirty: bool,
	offer_save: bool,
}

fn navigation_state(
	session: &DesktopSession,
	target: &ActiveTarget,
	can_save: bool,
) -> Result<NavigationState, CommandError> {
	let active = session.engine_for(target)?;
	Ok(NavigationState {
		dirty: active.dirty,
		offer_save: can_save && session.selected_for(target).is_ok() && can_create_bank_copy(),
	})
}

fn finish_navigation(
	session: &mut DesktopSession,
	target: ActiveTarget,
	choice: CloseChoice,
	offer_save: bool,
) -> Result<CloseChoice, CommandError> {
	let active = session.engine_for(&target)?;
	if choice == CloseChoice::Save && (!offer_save || session.selected_for(&target).is_err()) {
		return Err(CommandError::Unavailable);
	}
	if choice == CloseChoice::Discard {
		session.discard = Some(DiscardPermit {
			request_id: target.request_id,
			document_id: target.document_id,
			revision: active.revision,
		});
	}
	Ok(choice)
}

#[tauri::command]
pub(crate) async fn confirm_bank_navigation(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
	can_save: bool,
) -> Result<CloseChoice, CommandError> {
	let _dialog = state.dialog()?;
	let preview = {
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let preview = navigation_state(&session, &target, can_save)?;
		if !preview.dirty {
			return finish_navigation(
				&mut session,
				target,
				CloseChoice::Discard,
				preview.offer_save,
			);
		}
		preview
	};
	let title = if preview.offer_save {
		"Save changes before opening another bank?"
	} else {
		"Discard changes and open another bank?"
	};
	let choice = ask_unsaved(
		&window,
		title,
		"The current bank has unsaved changes.",
		preview.offer_save,
	)
	.await;
	let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
	finish_navigation(&mut session, target, choice, preview.offer_save)
}

#[cfg(test)]
mod tests;
