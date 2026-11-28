use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::json;
use starbank_editor::{NativeActiveTarget, PROTOCOL_VERSION};
use tauri::{AppHandle, Emitter as _, Manager as _, RunEvent, State, WebviewWindow, WindowEvent};

use crate::commands::{CommandError, DesktopEngine};

mod unsaved;

pub(crate) use unsaved::ask_unsaved;

const MAIN_WINDOW: &str = "main";
const REQUEST_EVENT: &str = "bank-close-requested";
const REQUEST_ID: &str = "native-close";
#[cfg(target_os = "macos")]
const QUIT_ITEM: &str = "starbank-guarded-quit";

#[derive(Default)]
pub(crate) struct DesktopCloseGuard(Mutex<Guard>);

#[derive(Default)]
struct Guard {
	last_ticket: u32,
	pending: Option<Attempt>,
	allow_exit: bool,
}

#[derive(Clone, Copy)]
enum Intent {
	Window,
	Exit(i32),
}

struct Attempt {
	ticket: u32,
	intent: Intent,
	captured: Option<NativeActiveTarget>,
	phase: Phase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
	Probe,
	Prompt,
	Approved {
		choice: CloseChoice,
		draft_generation: u32,
	},
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseChoice {
	Cancel,
	Discard,
	Save,
}

#[derive(Clone, Serialize)]
struct CloseProbe {
	ticket: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CloseReport {
	ticket: u32,
	draft_generation: u32,
	has_draft: bool,
	can_save: bool,
}

impl Guard {
	fn begin(
		&mut self,
		intent: Intent,
		captured: Option<NativeActiveTarget>,
	) -> Option<CloseProbe> {
		if self
			.pending
			.as_ref()
			.is_some_and(|attempt| attempt.phase != Phase::Probe)
		{
			return None;
		}
		let ticket = self.last_ticket.checked_add(1)?;
		self.last_ticket = ticket;
		self.pending = Some(Attempt {
			ticket,
			intent,
			captured,
			phase: Phase::Probe,
		});
		Some(CloseProbe { ticket })
	}

	fn attempt(&mut self, ticket: u32) -> Option<&mut Attempt> {
		self.pending
			.as_mut()
			.filter(|attempt| attempt.ticket == ticket)
	}

	fn clear(&mut self, ticket: u32) {
		if self.attempt(ticket).is_some() {
			self.pending = None;
		}
	}
}

impl Attempt {
	fn permits(&self, current: Option<&NativeActiveTarget>, draft_generation: u32) -> bool {
		let Phase::Approved {
			choice,
			draft_generation: expected,
		} = self.phase
		else {
			return false;
		};
		if expected != draft_generation {
			return false;
		}
		match choice {
			CloseChoice::Cancel => false,
			CloseChoice::Discard => current == self.captured.as_ref(),
			CloseChoice::Save => {
				self.captured
					.as_ref()
					.zip(current)
					.is_some_and(|(captured, active)| {
						active.document_id == captured.document_id && !active.dirty
					})
			}
		}
	}
}

pub(crate) fn handle_event(app: &AppHandle, event: &RunEvent) {
	match event {
		RunEvent::WindowEvent {
			label,
			event: WindowEvent::CloseRequested { api, .. },
			..
		} if label == MAIN_WINDOW => {
			api.prevent_close();
			request(app, Intent::Window);
		}
		RunEvent::ExitRequested { api, code, .. }
			if app.get_webview_window(MAIN_WINDOW).is_some() =>
		{
			let guard = app.state::<DesktopCloseGuard>();
			if guard
				.0
				.lock()
				.is_ok_and(|mut state| std::mem::take(&mut state.allow_exit))
			{
				return;
			}
			api.prevent_exit();
			request(app, Intent::Exit(code.unwrap_or_default()));
		}
		#[cfg(target_os = "macos")]
		RunEvent::MenuEvent(event) if event.id() == QUIT_ITEM => request(app, Intent::Exit(0)),
		_ => {}
	}
}

fn request(app: &AppHandle, intent: Intent) {
	let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
		return;
	};
	let engine = app.state::<DesktopEngine>();
	let Ok(_dialog) = engine.dialog() else {
		return;
	};
	let Ok(session) = engine.engine.try_lock() else {
		return;
	};
	let captured = session.editor.native_active_target();
	let guard = app.state::<DesktopCloseGuard>();
	// Before the first accepted bank, the webview cannot hold a bank draft.
	if captured.is_none() && !session.editor.native_has_opened_document() {
		let _ = close_native(&window, &guard, intent);
		return;
	}
	drop(session);
	let probe = guard
		.0
		.lock()
		.ok()
		.and_then(|mut state| state.begin(intent, captured));
	if let Some(probe) = probe
		&& window
			.emit_to(MAIN_WINDOW, REQUEST_EVENT, probe.clone())
			.is_err()
		&& let Ok(mut state) = guard.0.lock()
	{
		state.clear(probe.ticket);
	}
}

#[tauri::command]
pub(crate) async fn confirm_window_close(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
	guard: State<'_, DesktopCloseGuard>,
	report: CloseReport,
) -> Result<CloseChoice, CommandError> {
	require_main(&window)?;
	let _dialog = engine.dialog()?;
	let active = engine
		.engine
		.try_lock()
		.map_err(|_| CommandError::Unavailable)?
		.editor
		.native_active_target();
	let dirty = {
		let mut state = guard.0.lock().map_err(|_| CommandError::Unavailable)?;
		let attempt = state
			.attempt(report.ticket)
			.ok_or(CommandError::Unavailable)?;
		if attempt.phase != Phase::Probe {
			return Err(CommandError::Unavailable);
		}
		if active.as_ref() != attempt.captured.as_ref() {
			state.clear(report.ticket);
			return Err(stale());
		}
		attempt.phase = Phase::Prompt;
		attempt.captured.as_ref().is_some_and(|target| target.dirty)
	};

	let can_save = active.is_some() && report.can_save;
	let choice = ask(&window, dirty, report.has_draft, can_save).await;
	let mut state = guard.0.lock().map_err(|_| CommandError::Unavailable)?;
	if choice == CloseChoice::Cancel {
		state.clear(report.ticket);
	} else {
		let attempt = state
			.attempt(report.ticket)
			.ok_or(CommandError::Unavailable)?;
		attempt.phase = Phase::Approved {
			choice,
			draft_generation: report.draft_generation,
		};
	}
	Ok(choice)
}

#[tauri::command]
pub(crate) fn complete_window_close(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
	guard: State<'_, DesktopCloseGuard>,
	ticket: u32,
	draft_generation: u32,
	proceed: bool,
) -> Result<(), CommandError> {
	require_main(&window)?;
	// Consume the decision even when a newly started operation prevents closing.
	let attempt = {
		let mut state = guard.0.lock().map_err(|_| CommandError::Unavailable)?;
		state.attempt(ticket).ok_or(CommandError::Unavailable)?;
		state.pending.take().ok_or(CommandError::Unavailable)?
	};
	if !proceed {
		return Ok(());
	}
	let _dialog = engine.dialog()?;
	let mut session = engine
		.engine
		.try_lock()
		.map_err(|_| CommandError::Unavailable)?;
	let active = session.editor.native_active_target();
	if !attempt.permits(active.as_ref(), draft_generation) {
		return Err(stale());
	}
	let Some(active) = active else {
		// Keep the empty engine locked through the native close request.
		return close_native(&window, &guard, attempt.intent);
	};
	let control = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": REQUEST_ID,
		"document_id": active.document_id,
		"expected_revision": active.revision.to_string(),
		"command": {"kind": "close", "payload": {"discard": true}},
	});
	let reply = session.dispatch(&control.to_string(), None);
	let reply: serde_json::Value =
		serde_json::from_str(&reply.json).map_err(|_| CommandError::InvalidPacket)?;
	if reply["result"]["kind"] != "closed" {
		return Err(stale());
	}
	drop(session);
	close_native(&window, &guard, attempt.intent)
}

fn close_native(
	window: &WebviewWindow,
	guard: &DesktopCloseGuard,
	intent: Intent,
) -> Result<(), CommandError> {
	match intent {
		Intent::Window => window.destroy().map_err(|_| CommandError::Unavailable),
		Intent::Exit(code) => {
			guard
				.0
				.lock()
				.map_err(|_| CommandError::Unavailable)?
				.allow_exit = true;
			window.app_handle().exit(code);
			Ok(())
		}
	}
}

fn require_main(window: &WebviewWindow) -> Result<(), CommandError> {
	if window.label() == MAIN_WINDOW {
		Ok(())
	} else {
		Err(CommandError::Unavailable)
	}
}

fn stale() -> CommandError {
	CommandError::ActiveWriteRejected("The bank changed before the window could close.".into())
}

async fn ask(window: &WebviewWindow, dirty: bool, has_draft: bool, can_save: bool) -> CloseChoice {
	if !dirty && !has_draft {
		return CloseChoice::Discard;
	}
	let offer_save = can_save && !has_draft && cfg!(any(target_os = "macos", target_os = "linux"));
	let description = if has_draft {
		"The editor has unapplied changes. To save them, cancel and apply the draft first."
	} else {
		"The current bank has unsaved changes."
	};
	let title = if offer_save {
		"Save changes before closing?"
	} else {
		"Discard changes and close?"
	};
	ask_unsaved(window, title, description, offer_save).await
}

#[cfg(target_os = "macos")]
pub(crate) fn menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
	use tauri::menu::{Menu, MenuItem, MenuItemKind};

	// The pinned Tauri default's application submenu ends in AppKit's direct Quit item.
	let menu = Menu::default(app)?;
	if let Some(MenuItemKind::Submenu(application)) = menu.items()?.first()
		&& let Some(item @ MenuItemKind::Predefined(_)) = application.items()?.last()
	{
		application.remove(item)?;
		application.append(&MenuItem::with_id(
			app,
			QUIT_ITEM,
			format!("Quit {}", app.package_info().name),
			true,
			Some("CmdOrCtrl+Q"),
		)?)?;
	}
	Ok(menu)
}

#[cfg(test)]
mod tests {
	use starbank_core::Revision;

	use super::*;

	fn target(id: &str, revision: &str, dirty: bool) -> NativeActiveTarget {
		NativeActiveTarget {
			document_id: id.to_owned(),
			revision: revision.parse::<Revision>().expect("synthetic revision"),
			dirty,
		}
	}

	#[test]
	fn discard_requires_current_bank_and_draft_generation() {
		let active = target("bank-1", "4", true);
		let mut guard = Guard::default();
		let probe = guard
			.begin(Intent::Window, Some(active.clone()))
			.expect("new probe");
		let attempt = guard.attempt(probe.ticket).expect("pending");
		assert!(!attempt.permits(Some(&active), 7));
		attempt.phase = Phase::Approved {
			choice: CloseChoice::Discard,
			draft_generation: 7,
		};
		assert!(attempt.permits(Some(&active), 7));
		assert!(!attempt.permits(Some(&active), 8));
		assert!(!attempt.permits(Some(&target("bank-1", "5", true)), 7));
		assert!(!attempt.permits(Some(&target("bank-2", "4", true)), 7));
		assert!(!attempt.permits(None, 7));
	}

	#[test]
	fn save_requires_a_clean_checkpoint_after_confirmation() {
		let active = target("bank-1", "4", true);
		let attempt = Attempt {
			ticket: 1,
			intent: Intent::Window,
			captured: Some(active.clone()),
			phase: Phase::Approved {
				choice: CloseChoice::Save,
				draft_generation: 7,
			},
		};
		assert!(!attempt.permits(Some(&active), 7));
		assert!(attempt.permits(Some(&target("bank-1", "5", false)), 7));
		assert!(!attempt.permits(Some(&target("bank-1", "6", true)), 7));
		assert!(!attempt.permits(Some(&target("bank-2", "5", false)), 7));
		assert!(!attempt.permits(Some(&target("bank-1", "5", false)), 8));
	}

	#[test]
	fn no_engine_still_requires_approval_and_an_unchanged_draft() {
		let mut guard = Guard::default();
		let probe = guard.begin(Intent::Window, None).expect("new probe");
		let attempt = guard.attempt(probe.ticket).expect("pending");
		assert!(!attempt.permits(None, 7));
		attempt.phase = Phase::Approved {
			choice: CloseChoice::Cancel,
			draft_generation: 7,
		};
		assert!(!attempt.permits(None, 7));
		attempt.phase = Phase::Approved {
			choice: CloseChoice::Discard,
			draft_generation: 7,
		};
		assert!(attempt.permits(None, 7));
		assert!(!attempt.permits(None, 8));
		assert!(!attempt.permits(Some(&target("late-bank", "0", false)), 7));
		attempt.phase = Phase::Approved {
			choice: CloseChoice::Save,
			draft_generation: 7,
		};
		assert!(!attempt.permits(None, 7));
		assert!(!attempt.permits(Some(&target("late-bank", "0", false)), 7));
	}

	#[test]
	fn retried_probes_cannot_reuse_previous_approvals() {
		let active = target("bank-1", "4", true);
		let mut guard = Guard::default();
		let first = guard.begin(Intent::Window, Some(active.clone())).unwrap();
		let second = guard.begin(Intent::Exit(0), Some(active.clone())).unwrap();
		assert_ne!(first.ticket, second.ticket);
		assert!(guard.attempt(first.ticket).is_none());
		guard.attempt(second.ticket).unwrap().phase = Phase::Prompt;
		assert!(guard.begin(Intent::Window, Some(active)).is_none());
		guard.clear(first.ticket);
		assert!(guard.attempt(second.ticket).is_some());
		guard.clear(second.ticket);
		assert!(guard.pending.is_none());
	}
}
