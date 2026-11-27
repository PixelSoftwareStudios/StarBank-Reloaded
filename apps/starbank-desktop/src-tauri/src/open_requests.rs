use std::{
	collections::VecDeque,
	ffi::OsString,
	path::{Path, PathBuf},
	sync::Mutex,
};

#[cfg(windows)]
use std::{sync::TryLockError, time::Instant};

use serde::Serialize;
#[cfg(windows)]
use starbank_instance::{MAX_PATHS, QueueAdmission};
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

enum PathAdmission {
	Queued,
	AlreadyPending,
	Refused,
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

	#[cfg(windows)]
	fn try_admit(
		&self,
		paths: Vec<PathBuf>,
		deadline: Instant,
	) -> Result<QueueAdmission, CommandError> {
		if paths.len() > MAX_PATHS {
			return Err(CommandError::InvalidControl);
		}
		let submitted = u8::try_from(paths.len()).map_err(|_| CommandError::InvalidControl)?;
		// Check the cutoff with the queue locked, immediately before admitting candidates.
		let (queued, already_pending, refused) = match self.0.try_lock() {
			Ok(mut queue) if Instant::now() < deadline => {
				let mut queued = 0;
				let mut already_pending = 0;
				let mut refused = 0;
				for path in paths {
					match queue.admit_path(path) {
						PathAdmission::Queued => queued += 1,
						PathAdmission::AlreadyPending => already_pending += 1,
						PathAdmission::Refused => refused += 1,
					}
				}
				(queued, already_pending, refused)
			}
			Ok(_) | Err(TryLockError::WouldBlock) => (0, 0, submitted),
			Err(TryLockError::Poisoned(_)) => return Err(CommandError::Unavailable),
		};
		QueueAdmission::new(queued, already_pending, refused)
			.map_err(|_| CommandError::InvalidControl)
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
			if self.requests.len() >= QUEUE_LIMIT || self.next_id == u32::MAX {
				break;
			}
			changed |= matches!(self.admit_path(path), PathAdmission::Queued);
		}
		changed
	}

	fn admit_path(&mut self, path: PathBuf) -> PathAdmission {
		if !is_local_candidate(&path) {
			return PathAdmission::Refused;
		}
		if self.requests.iter().any(|pending| pending.path == path) {
			return PathAdmission::AlreadyPending;
		}
		if self.requests.len() >= QUEUE_LIMIT {
			return PathAdmission::Refused;
		}
		let Some(id) = self.next_id.checked_add(1) else {
			return PathAdmission::Refused;
		};
		let Ok(name) = selection::bank_name(&path) else {
			return PathAdmission::Refused;
		};
		self.next_id = id;
		self.requests.push_back(PendingOpen {
			summary: OpenRequest { id, name },
			path,
		});
		PathAdmission::Queued
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

#[cfg(windows)]
pub(crate) fn admit_running_instance<R: Runtime>(
	app: &tauri::AppHandle<R>,
	paths: Vec<PathBuf>,
	deadline: Instant,
) -> Result<QueueAdmission, CommandError> {
	let state = app
		.try_state::<OpenRequests>()
		.ok_or(CommandError::Unavailable)?;
	let admission = state.try_admit(paths, deadline)?;
	if admission.queued() != 0 {
		notify(app);
	}
	Ok(admission)
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
	fn cutoff() -> Instant {
		Instant::now() + std::time::Duration::from_secs(30)
	}

	#[cfg(windows)]
	fn counts(admission: QueueAdmission) -> (u8, u8, u8) {
		(
			admission.queued(),
			admission.already_pending(),
			admission.refused(),
		)
	}

	#[cfg(windows)]
	#[test]
	fn warm_admission_counts_duplicates_before_and_after_capacity_is_reached() {
		let folder = Path::new(TEST_FOLDER);
		let state = OpenRequests::from_args(
			Some(folder),
			(0..QUEUE_LIMIT - 1).map(|number| OsString::from(format!("{number}.SC2Bank"))),
		);
		let new_bank = folder.join("New.SC2Bank");
		let paths = vec![
			folder.join("0.SC2Bank"),
			folder.join("notes.txt"),
			PathBuf::from("Relative.SC2Bank"),
			new_bank.clone(),
			new_bank.clone(),
			folder.join("Full.SC2Bank"),
			folder.join(format!("{}.SC2Bank", QUEUE_LIMIT - 2)),
		];
		let submitted = paths.len();
		let admission = state.try_admit(paths, cutoff()).expect("valid admission");
		assert_eq!(counts(admission), (1, 3, 3));
		assert_eq!(admission.total(), submitted);
		let queue = state.0.lock().expect("admission released queue");
		assert_eq!(queue.requests.len(), QUEUE_LIMIT);
		assert_eq!(
			queue.requests.front().expect("earlier entry").summary.name,
			"0.SC2Bank"
		);
		assert_eq!(queue.requests.back().expect("new entry").path, new_bank);
	}

	#[cfg(windows)]
	#[test]
	fn warm_admission_never_reuses_ids_and_still_counts_existing_duplicates() {
		let folder = Path::new(TEST_FOLDER);
		let state = OpenRequests(Mutex::new(Queue {
			next_id: u32::MAX - 1,
			..Queue::default()
		}));
		let last = folder.join("Last.SC2Bank");
		let admission = state
			.try_admit(
				vec![last.clone(), folder.join("Overflow.SC2Bank"), last.clone()],
				cutoff(),
			)
			.expect("bounded final ID");
		assert_eq!(counts(admission), (1, 1, 1));
		assert_eq!(
			state.take(u32::MAX).expect("last unique ID"),
			Some(last.clone())
		);
		let admission = state
			.try_admit(vec![last], cutoff())
			.expect("exhausted queue");
		assert_eq!(counts(admission), (0, 0, 1));
		assert!(state.0.lock().expect("queue").requests.is_empty());
	}

	#[cfg(windows)]
	#[test]
	fn warm_admission_refuses_contention_and_expiry_without_enqueuing() {
		let state = OpenRequests::default();
		let bank = Path::new(TEST_FOLDER).join("Candidate.SC2Bank");
		let held = state.0.lock().expect("hold queue");
		let busy = state
			.try_admit(vec![bank.clone()], cutoff())
			.expect("busy admission");
		assert_eq!(counts(busy), (0, 0, 1));
		assert!(held.requests.is_empty());
		drop(held);

		let late = state
			.try_admit(vec![bank.clone()], Instant::now())
			.expect("expired admission");
		assert_eq!(counts(late), (0, 0, 1));
		let admitted = state
			.try_admit(vec![bank.clone()], cutoff())
			.expect("fresh admission");
		assert_eq!(counts(admitted), (1, 0, 0));
		assert_eq!(state.take(1).expect("first request"), Some(bank));
	}

	#[cfg(windows)]
	#[test]
	fn warm_admission_checks_protocol_bounds_and_accepts_activation_only() {
		let state = OpenRequests::default();
		let bank = Path::new(TEST_FOLDER).join("Candidate.SC2Bank");
		assert!(matches!(
			state.try_admit(vec![bank.clone(); MAX_PATHS + 1], cutoff()),
			Err(CommandError::InvalidControl)
		));
		let focus = state
			.try_admit(Vec::new(), cutoff())
			.expect("activation admission");
		assert_eq!(counts(focus), (0, 0, 0));
		assert!(state.0.lock().expect("queue").requests.is_empty());
		let at_limit = state
			.try_admit(vec![bank; MAX_PATHS], cutoff())
			.expect("request at path limit");
		assert_eq!(at_limit.total(), MAX_PATHS);
		assert_eq!(at_limit.queued(), 1);
		assert_eq!(usize::from(at_limit.already_pending()), MAX_PATHS - 1);
		assert_eq!(at_limit.refused(), 0);
	}

	#[cfg(windows)]
	#[test]
	fn warm_admission_reports_a_poisoned_queue_as_an_error() {
		let state = OpenRequests::default();
		std::thread::scope(|scope| {
			let holder = scope.spawn(|| {
				let _held = state.0.lock().expect("hold queue");
				panic!("poison the queue for this fixture");
			});
			assert!(holder.join().is_err());
		});
		assert!(matches!(
			state.try_admit(Vec::new(), cutoff()),
			Err(CommandError::Unavailable)
		));
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
