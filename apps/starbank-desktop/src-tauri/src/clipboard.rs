//! Publishes only revision-checked bank XML to the Linux clipboard.

#[cfg(target_os = "linux")]
use tauri::Manager as _;
use tauri::WebviewWindow;

use crate::commands::CommandError;

#[cfg(any(target_os = "linux", test))]
use {
	sc2bank::{Limits, parse},
	serde::Deserialize,
	starbank_editor::{MAX_CONTROL_BYTES, WireReply},
};

#[cfg(any(target_os = "linux", test))]
#[derive(Deserialize)]
struct PrepareEnvelope {
	command: PrepareCommand,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareCommand {
	kind: PrepareKind,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PrepareKind {
	Prepare,
}

#[cfg(any(target_os = "linux", test))]
fn validate_copy_control(control: &str) -> Result<(), CommandError> {
	if control.len() > MAX_CONTROL_BYTES {
		return Err(CommandError::InvalidControl);
	}
	let PrepareEnvelope {
		command: PrepareCommand {
			kind: PrepareKind::Prepare,
		},
	} = serde_json::from_str(control).map_err(|_| CommandError::InvalidControl)?;
	Ok(())
}

#[cfg(any(target_os = "linux", test))]
#[derive(Deserialize)]
struct PrepareReply {
	result: PrepareResult,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PrepareResult {
	Error,
	Prepared,
}

#[cfg(any(target_os = "linux", test))]
fn clipboard_bytes(
	reply: &WireReply,
	allow_unverified: bool,
) -> Result<Option<Vec<u8>>, CommandError> {
	let reply_kind: PrepareReply =
		serde_json::from_str(&reply.json).map_err(|_| CommandError::InvalidPacket)?;
	let bytes = match (reply_kind.result, &reply.bytes) {
		(PrepareResult::Error, None) => return Ok(None),
		(PrepareResult::Prepared, Some(bytes)) => bytes,
		_ => return Err(CommandError::InvalidPacket),
	};
	if crate::export::needs_confirmation(&reply.json)? && !allow_unverified {
		return Err(CommandError::ClipboardConsentRequired);
	}
	let bank = parse(bytes, Limits::default()).map_err(|_| CommandError::InvalidPacket)?;
	Ok(Some(bank.xml_text().as_bytes().to_vec()))
}

/// Copies prepared XML from the requested editor revision on Linux.
///
/// Returns the engine control response, or `None` on platforms using WebView clipboard access.
#[tauri::command]
pub(crate) async fn copy_bank_xml(
	window: WebviewWindow,
	control: String,
	allow_unverified: bool,
) -> Result<Option<String>, CommandError> {
	#[cfg(not(target_os = "linux"))]
	{
		let _ = (window, control, allow_unverified);
		Ok(None)
	}
	#[cfg(target_os = "linux")]
	{
		validate_copy_control(&control)?;
		let app = window.app_handle().clone();
		let (reply, xml) = tauri::async_runtime::spawn_blocking(move || {
			let reply = crate::export::prepare(&app, &control)?;
			let xml = clipboard_bytes(&reply, allow_unverified)?;
			Ok::<_, CommandError>((reply.json, xml))
		})
		.await
		.map_err(|_| CommandError::Unavailable)??;
		if let Some(xml) = xml {
			publish(&window, xml).await?;
		}
		Ok(Some(reply))
	}
}

#[cfg(target_os = "linux")]
async fn publish(window: &WebviewWindow, xml: Vec<u8>) -> Result<(), CommandError> {
	let (send, mut receive) = tauri::async_runtime::channel(1);
	window
		.with_webview(move |view| {
			let _ = send.try_send(claim_clipboard(view, xml));
		})
		.map_err(|_| CommandError::ClipboardUnavailable)?;
	receive
		.recv()
		.await
		.ok_or(CommandError::ClipboardUnavailable)?
}

#[cfg(target_os = "linux")]
fn claim_clipboard(
	view: tauri::webview::PlatformWebview,
	xml: Vec<u8>,
) -> Result<(), CommandError> {
	use gtk::{Clipboard, TargetEntry, TargetFlags, prelude::WidgetExt as _};

	const UTF8_TARGETS: [&str; 2] = ["UTF8_STRING", "text/plain;charset=utf-8"];
	const RAW_TEXT_TARGET: u32 = 0;
	const UNIT_BITS: i32 = 8;

	if !gtk::is_initialized_main_thread() {
		return Err(CommandError::ClipboardUnavailable);
	}
	let display = view.inner().display();
	let clipboard = Clipboard::default(&display).ok_or(CommandError::ClipboardUnavailable)?;
	let targets =
		UTF8_TARGETS.map(|target| TargetEntry::new(target, TargetFlags::empty(), RAW_TEXT_TARGET));
	// GTK's text helper can rewrite line endings when serving MIME text targets.
	// Raw selection data copies the prepared bytes, bounded by the XML parser's limits.
	let claimed = clipboard.set_with_data(&targets, move |_, selection, _| {
		selection.set(&selection.target(), UNIT_BITS, &xml);
	});
	if !claimed {
		return Err(CommandError::ClipboardUnavailable);
	}
	Ok(())
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
