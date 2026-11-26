use std::{
	collections::VecDeque,
	ffi::OsString,
	path::{Path, PathBuf},
	sync::Mutex,
};

use serde::Serialize;
use tauri::{Emitter as _, Manager as _, Runtime, State, WebviewWindow};

use crate::{
	commands::{CommandError, DesktopEngine, OPEN_REQUEST, OpenedBank, is_request},
	selection,
};

const QUEUE_LIMIT: usize = 32;
const QUEUE_CHANGED: &str = "bank-open-requests";

#[derive(Default)]
pub(crate) struct OpenRequests(Mutex<Queue>);

#[derive(Default)]
struct Queue {
	next_id: u32,
	requests: VecDeque<PendingOpen>,
}

struct PendingOpen {
	summary: OpenRequest,
	path: PathBuf,
}

#[derive(Clone, Serialize)]
pub(crate) struct OpenRequest {
	id: u32,
	name: String,
}

impl OpenRequests {
	pub(crate) fn from_args(
		current_dir: Option<&Path>,
		args: impl IntoIterator<Item = OsString>,
	) -> Self {
		let mut queue = Queue::default();
		queue.extend(args.into_iter().filter_map(|arg| {
			let path = PathBuf::from(arg);
			if path.is_absolute() {
				Some(path)
			} else {
				#[cfg(windows)]
				if path.has_root()
					|| matches!(
						path.components().next(),
						Some(std::path::Component::Prefix(_))
					) {
					return None;
				}
				current_dir.map(|directory| directory.join(path))
			}
		}));
		Self(Mutex::new(queue))
	}

	fn take(&self, id: u32) -> Result<Option<PathBuf>, CommandError> {
		let mut queue = self.0.lock().map_err(|_| CommandError::Unavailable)?;
		Ok(queue
			.requests
			.iter()
			.position(|pending| pending.summary.id == id)
			.and_then(|index| queue.requests.remove(index))
			.map(|pending| pending.path))
	}
}

impl Queue {
	fn extend(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> bool {
		let mut changed = false;
		for path in paths {
			if self.requests.len() >= QUEUE_LIMIT {
				break;
			}
			if !is_local_candidate(&path)
				|| self.requests.iter().any(|pending| pending.path == path)
			{
				continue;
			}
			let Ok(name) = selection::bank_name(&path) else {
				continue;
			};
			let Some(id) = self.next_id.checked_add(1) else {
				break;
			};
			self.next_id = id;
			self.requests.push_back(PendingOpen {
				summary: OpenRequest { id, name },
				path,
			});
			changed = true;
		}
		changed
	}
}

fn is_local_candidate(path: &Path) -> bool {
	#[cfg(windows)]
	{
		use std::path::{Component, Prefix};
		matches!(
			path.components().next(),
			Some(Component::Prefix(prefix))
				if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
					&& path.is_absolute()
		)
	}
	#[cfg(not(windows))]
	path.is_absolute()
}

pub(crate) fn enqueue<R: Runtime>(
	app: &tauri::AppHandle<R>,
	paths: impl IntoIterator<Item = PathBuf>,
) {
	let state = app.state::<OpenRequests>();
	let changed = state.0.lock().is_ok_and(|mut queue| queue.extend(paths));
	if changed {
		notify(app);
	}
}

fn notify<R: Runtime>(app: &tauri::AppHandle<R>) {
	let _ = app.emit_to("main", QUEUE_CHANGED, ());
}

#[tauri::command]
pub(crate) fn list_open_requests(
	requests: State<'_, OpenRequests>,
) -> Result<Vec<OpenRequest>, CommandError> {
	let queue = requests.0.lock().map_err(|_| CommandError::Unavailable)?;
	Ok(queue
		.requests
		.iter()
		.map(|pending| pending.summary.clone())
		.collect())
}

#[tauri::command]
pub(crate) fn dismiss_open_request(
	window: WebviewWindow,
	requests: State<'_, OpenRequests>,
	id: u32,
) -> Result<(), CommandError> {
	if requests.take(id)?.is_some() {
		notify(window.app_handle());
	}
	Ok(())
}

#[tauri::command]
pub(crate) async fn open_requested_bank(
	window: WebviewWindow,
	id: u32,
	control: String,
) -> Result<Option<OpenedBank>, CommandError> {
	if !is_request(&control, OPEN_REQUEST)? {
		return Err(CommandError::InvalidControl);
	}
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let mut engine = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let requests = app.state::<OpenRequests>();
		let Some(path) = requests.take(id)? else {
			return Ok(None);
		};
		notify(&app);
		engine.open_path(&control, &path).map(Some)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

#[cfg(test)]
mod tests {
	use super::*;

	#[cfg(not(windows))]
	const TEST_FOLDER: &str = "/synthetic/StarCraft II";
	#[cfg(windows)]
	const TEST_FOLDER: &str = r"C:\synthetic\StarCraft II";

	#[test]
	fn startup_paths_are_bounded_redacted_and_consumed_once() {
		let folder = Path::new(TEST_FOLDER);
		let state = OpenRequests::from_args(
			Some(folder),
			[
				OsString::from("One.Sc2Bank"),
				OsString::from("notes.txt"),
				OsString::from("One.Sc2Bank"),
				OsString::from("Two.SC2BANK"),
			],
		);
		let queue = state.0.lock().expect("request queue");
		let summaries: Vec<_> = queue
			.requests
			.iter()
			.map(|pending| &pending.summary)
			.collect();
		assert_eq!(
			serde_json::to_value(summaries).expect("redacted summaries"),
			serde_json::json!([
				{"id": 1, "name": "One.Sc2Bank"},
				{"id": 2, "name": "Two.SC2BANK"}
			])
		);
		drop(queue);
		assert_eq!(
			state.take(1).expect("first request"),
			Some(folder.join("One.Sc2Bank"))
		);
		assert_eq!(state.take(1).expect("consumed request"), None);
		assert_eq!(state.take(99).expect("unknown request"), None);
	}

	#[test]
	fn full_queue_keeps_earlier_requests() {
		let folder = Path::new(TEST_FOLDER);
		let mut queue = Queue::default();
		assert!(
			queue.extend(
				(0..QUEUE_LIMIT + 5).map(|number| folder.join(format!("{number}.SC2Bank")))
			)
		);
		assert!(!queue.extend([folder.join("later.SC2Bank")]));
		assert_eq!(queue.requests.len(), QUEUE_LIMIT);
		assert_eq!(
			queue.requests.front().expect("first request").summary.name,
			"0.SC2Bank"
		);
	}

	#[cfg(windows)]
	#[test]
	fn rejects_unc_and_drive_relative_paths() {
		for path in [
			r"\\host\share\Remote.SC2Bank",
			r"C:Relative.SC2Bank",
			r"\Rooted.SC2Bank",
		] {
			assert!(!is_local_candidate(Path::new(path)));
		}
	}
}
