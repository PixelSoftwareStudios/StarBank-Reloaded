use std::{
	ffi::OsString,
	io,
	os::windows::ffi::OsStrExt as _,
	path::{Component, Path, PathBuf},
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
	time::Instant,
};

use rfd::{MessageButtons, MessageDialog, MessageLevel};
use starbank_instance::{
	Claim, EndpointGuard, FrameError, InstanceError, Listener, MAX_FRAME_LEN, MAX_PATHS,
	OpenRequest, QueueAdmission, claim, is_bank_candidate,
};
use tauri::{AppHandle, Manager as _, Runtime, async_runtime};

use crate::{
	open_requests::{OpenRequests, admit_running_instance},
	selection::BANK_EXTENSION,
};

const MESSAGE_TITLE: &str = "StarBank Reloaded";

pub(crate) struct PrimaryLaunch {
	listener: Listener,
	endpoint: EndpointGuard,
}

struct StartupRequests {
	paths: Vec<PathBuf>,
	wire: OpenRequest,
	refused: usize,
}

impl StartupRequests {
	fn from_args(
		current_dir: Option<&Path>,
		args: impl IntoIterator<Item = OsString>,
	) -> Result<Self, FrameError> {
		let mut paths = Vec::new();
		let mut refused = 0;
		for argument in args {
			let path = PathBuf::from(argument);
			if !path
				.extension()
				.is_some_and(|extension| extension.eq_ignore_ascii_case(BANK_EXTENSION))
			{
				continue;
			}
			let Some(path) = resolve_candidate(current_dir, path) else {
				refused += 1;
				continue;
			};
			if paths.len() == MAX_PATHS {
				return Err(FrameError::TooManyPaths);
			}
			paths.push(path);
		}
		let wire = OpenRequest::new(
			paths
				.iter()
				.map(|path| path.as_os_str().encode_wide().collect())
				.collect(),
		)?;
		Ok(Self {
			paths,
			wire,
			refused,
		})
	}
}

fn resolve_candidate(current_dir: Option<&Path>, path: PathBuf) -> Option<PathBuf> {
	let absolute = if path.is_absolute() {
		path
	} else {
		if path.has_root() || matches!(path.components().next(), Some(Component::Prefix(_))) {
			return None;
		}
		current_dir?.join(path)
	};
	is_bank_candidate(&absolute).then_some(absolute)
}

pub(crate) fn begin(namespace: &str) -> Option<(OpenRequests, PrimaryLaunch)> {
	let current_dir = std::env::current_dir().ok();
	let startup =
		match StartupRequests::from_args(current_dir.as_deref(), std::env::args_os().skip(1)) {
			Ok(startup) => startup,
			Err(FrameError::TooManyPaths | FrameError::FrameTooLarge) => {
				show_message(
					MessageLevel::Warning,
					format!(
						"A launch supports up to {MAX_PATHS} bank paths in a {} KiB request.",
						MAX_FRAME_LEN / 1024
					),
				);
				return None;
			}
			Err(_) => {
				show_message(
					MessageLevel::Error,
					"The bank paths in this launch could not be read.",
				);
				return None;
			}
		};
	match async_runtime::block_on(claim(namespace, &startup.wire)) {
		Ok(Claim::Primary(listener)) => {
			let Ok(endpoint) = listener.retain_endpoint() else {
				show_message(
					MessageLevel::Error,
					"StarBank Reloaded could not retain its Windows launch endpoint.",
				);
				return None;
			};
			if startup.refused != 0 {
				show_message(
					MessageLevel::Warning,
					format!(
						"{} bank paths could not be queued. Use unambiguous local drive paths for Windows file launches.",
						startup.refused
					),
				);
			}
			let requests = OpenRequests::from_args(
				None,
				startup.paths.into_iter().map(PathBuf::into_os_string),
			);
			Some((requests, PrimaryLaunch { listener, endpoint }))
		}
		Ok(Claim::Forwarded(admission)) => {
			report_refusals(admission, startup.refused);
			None
		}
		Err(InstanceError::DeliveryUncertain { .. }) => {
			show_message(
				MessageLevel::Warning,
				"The running editor did not confirm receipt. Check its pending file requests before trying again.",
			);
			None
		}
		Err(_) => {
			show_message(
				MessageLevel::Error,
				"StarBank Reloaded could not establish a local launch connection.",
			);
			None
		}
	}
}

fn report_refusals(admission: QueueAdmission, invalid_paths: usize) {
	let refused = usize::from(admission.refused()) + invalid_paths;
	if refused != 0 {
		show_message(
			MessageLevel::Warning,
			format!(
				"The running editor queued {} bank requests. {} were already pending. {refused} could not be queued.",
				admission.queued(),
				admission.already_pending()
			),
		);
	}
}

fn show_message(level: MessageLevel, description: impl Into<String>) {
	let _ = MessageDialog::new()
		.set_title(MESSAGE_TITLE)
		.set_description(description)
		.set_level(level)
		.set_buttons(MessageButtons::Ok)
		.show();
}

struct ActiveInstance {
	task: async_runtime::JoinHandle<()>,
	_endpoint: EndpointGuard,
}

impl Drop for ActiveInstance {
	fn drop(&mut self) {
		self.task.abort();
	}
}

struct InstanceState(Mutex<Option<ActiveInstance>>);

pub(crate) fn start<R: Runtime>(app: &AppHandle<R>, primary: PrimaryLaunch, label: &'static str) {
	let PrimaryLaunch { listener, endpoint } = primary;
	let handle = app.clone();
	let task = async_runtime::spawn(async move {
		let pending_focus = Arc::new(AtomicBool::new(false));
		// The managed endpoint outlives the server task so the active editor retains ownership.
		let _ = listener
			.serve(move |paths, deadline| {
				let admission = admit_running_instance(&handle, paths, deadline)
					.map_err(|_| io::Error::other("instance launch queue is unavailable"))?;
				if Instant::now() < deadline
					&& (admission.total() == 0
						|| admission.queued() != 0
						|| admission.already_pending() != 0)
				{
					focus_main(&handle, label, &pending_focus);
				}
				Ok(admission)
			})
			.await;
	});
	app.manage(InstanceState(Mutex::new(Some(ActiveInstance {
		task,
		_endpoint: endpoint,
	}))));
}

fn focus_main<R: Runtime>(app: &AppHandle<R>, label: &'static str, pending: &Arc<AtomicBool>) {
	if pending.swap(true, Ordering::AcqRel) {
		return;
	}
	let handle = app.clone();
	let focus = Arc::clone(pending);
	if app
		.run_on_main_thread(move || {
			if let Some(window) = handle.get_webview_window(label) {
				let _ = window.unminimize();
				let _ = window.show();
				let _ = window.set_focus();
			}
			focus.store(false, Ordering::Release);
		})
		.is_err()
	{
		pending.store(false, Ordering::Release);
	}
}

pub(crate) fn stop<R: Runtime>(app: &AppHandle<R>) {
	if let Some(state) = app.try_state::<InstanceState>()
		&& let Ok(mut running) = state.0.lock()
	{
		drop(running.take());
	}
}

#[cfg(test)]
mod tests {
	use std::{os::windows::ffi::OsStringExt as _, process, thread, time::Duration};

	use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

	use super::*;
	use crate::open_requests::list_open_requests;

	struct StopOnDrop<'a>(&'a AppHandle<MockRuntime>);

	impl Drop for StopOnDrop<'_> {
		fn drop(&mut self) {
			stop(self.0);
		}
	}

	#[test]
	fn running_instance_updates_mock_app_queue_and_releases_endpoint() {
		const MAIN_LABEL: &str = "main";
		const STOP_BUDGET: Duration = Duration::from_secs(5);
		const STOP_POLL_INTERVAL: Duration = Duration::from_millis(10);

		let namespace = format!("org.starbank.desktop-test.{}", process::id());
		let activation = OpenRequest::new(Vec::new()).expect("focus-only request");
		let initial = async_runtime::block_on(claim(&namespace, &activation));
		let Claim::Primary(listener) = initial.expect("unique fixture must claim its pipe") else {
			panic!("unexpected running instance in the synthetic namespace");
		};
		let endpoint = listener.retain_endpoint().expect("retain primary endpoint");
		let app = mock_builder()
			.manage(OpenRequests::default())
			.build(mock_context(noop_assets()))
			.expect("windowless Tauri test app");
		assert!(app.webview_windows().is_empty());
		let cleanup = StopOnDrop(app.handle());
		start(cleanup.0, PrimaryLaunch { listener, endpoint }, MAIN_LABEL);

		let request = StartupRequests::from_args(
			None,
			[
				r"C:\StarBank fixture\Warm Launch 星.SC2Bank",
				r"D:\StarBank fixture\Second.Sc2Bank",
			]
			.map(OsString::from),
		)
		.expect("synthetic bank paths");
		let reply = async_runtime::block_on(claim(&namespace, &request.wire))
			.expect("running-instance acknowledgement");
		let Claim::Forwarded(admission) = reply else {
			panic!("an active app must retain the primary endpoint");
		};
		assert_eq!(
			(
				admission.queued(),
				admission.already_pending(),
				admission.refused()
			),
			(2, 0, 0)
		);
		let queued = list_open_requests(app.state()).expect("read the app's real queue");
		assert_eq!(
			serde_json::to_value(queued).expect("serialized command result"),
			serde_json::json!([
				{"id": 1, "name": "Warm Launch 星.SC2Bank"},
				{"id": 2, "name": "Second.Sc2Bank"}
			]),
		);
		assert!(app.webview_windows().is_empty());

		let state = app.state::<InstanceState>();
		let task = state
			.0
			.lock()
			.expect("managed primary")
			.as_ref()
			.expect("active launch state")
			.task
			.inner()
			.abort_handle();
		assert!(!task.is_finished());
		stop(cleanup.0);
		assert!(state.0.lock().expect("managed primary").is_none());
		let deadline = Instant::now() + STOP_BUDGET;
		while !task.is_finished() {
			assert!(
				Instant::now() < deadline,
				"listener cancellation deadline elapsed"
			);
			thread::sleep(STOP_POLL_INTERVAL);
		}

		let next = async_runtime::block_on(claim(&namespace, &activation));
		assert!(
			matches!(next, Ok(Claim::Primary(_))),
			"stopped app must release its endpoint"
		);
	}

	#[test]
	fn resolves_relative_paths_against_sender_and_refuses_ambiguous_paths() -> Result<(), FrameError>
	{
		let cwd = Path::new(r"C:\Sender Folder");
		let startup = StartupRequests::from_args(
			Some(cwd),
			[
				"Relative.SC2Bank",
				r"D:\Other Folder\Absolute.sc2bank",
				r"\RootRelative.SC2Bank",
				r"D:DriveRelative.SC2Bank",
				r"\\server\share\Remote.SC2Bank",
				r"..\Parent.SC2Bank",
				"notes.txt",
			]
			.map(OsString::from),
		)?;
		assert_eq!(
			startup.paths,
			[
				cwd.join("Relative.SC2Bank"),
				PathBuf::from(r"D:\Other Folder\Absolute.sc2bank")
			]
		);
		assert_eq!(startup.refused, 4);
		assert_eq!(startup.wire.paths().len(), 2);
		Ok(())
	}

	#[test]
	fn preserves_windows_code_units_and_bounds_the_request() -> Result<(), FrameError> {
		let mut exact: Vec<_> = r"C:\Sender\Unicode-星-".encode_utf16().collect();
		exact.push(0xD800);
		exact.extend(".Sc2Bank".encode_utf16());
		let startup = StartupRequests::from_args(None, [OsString::from_wide(&exact)])?;
		assert_eq!(startup.wire.paths(), [exact]);
		assert_eq!(startup.refused, 0);
		assert!(matches!(
			StartupRequests::from_args(
				Some(Path::new(r"C:\Sender")),
				(0..=MAX_PATHS).map(|index| OsString::from(format!("{index}.SC2Bank")))
			),
			Err(FrameError::TooManyPaths)
		));
		Ok(())
	}
}
