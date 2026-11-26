use tauri::{Manager as _, WebviewWindow};

use crate::{
	commands::{CommandError, DesktopEngine, OpenedBank},
	save::{ActiveTarget, write_rejected},
	selection::destination_context,
};

#[tauri::command]
pub(crate) async fn resign_active_bank(
	window: WebviewWindow,
	target: ActiveTarget,
) -> Result<OpenedBank, CommandError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let (selected, expected) = session.selected_for(&target)?;
		let path = selected.path.clone();
		let context = destination_context(&path).ok_or_else(|| {
			write_rejected("The bank path does not identify its signing context.")
		})?;
		let revision = session
			.editor
			.resign_native_active(&target.document_id, expected, &context)
			.map_err(write_rejected)?;
		session
			.snapshot(&target.request_id, target.document_id, revision, &path)
			.map_err(|_| CommandError::InvalidPacket)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}
