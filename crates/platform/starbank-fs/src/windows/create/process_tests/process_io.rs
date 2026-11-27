//! Bounded test-child handshake and lifetime. No handle or process outside this spawn is used.
use super::{
	FIXTURE_ENV, FRAME_PREFIX, MAX_FRAME_BYTES, MAX_STDOUT_BYTES, MAX_STDOUT_LINES, NONCE_ENV,
	Nonce, Phase, ReadyFrame, START_BYTE, WORKER_TEST,
};
use std::io::{self, BufRead as _, Read as _, Write as _};
use std::mem::size_of;
use std::os::windows::{
	io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
	process::CommandExt as _,
};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::ptr;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_sys::Win32::System::JobObjects::{
	AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
	JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
	SetInformationJobObject,
};

use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
const REAP_TIMEOUT: Duration = Duration::from_secs(10);
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

pub(super) struct Worker {
	child: Child,
	input: Option<ChildStdin>,
	job: Option<OwnedHandle>,
	reader: Option<JoinHandle<()>>,
	ready: Receiver<io::Result<ReadyFrame>>,
	reaped: bool,
}

impl Worker {
	pub(super) fn spawn(root: &Path, nonce: Nonce, phase: Phase) -> io::Result<Self> {
		let job = create_job()?;
		let child = Command::new(std::env::current_exe()?)
			.creation_flags(CREATE_NO_WINDOW)
			.args([
				"--ignored",
				"--exact",
				WORKER_TEST,
				"--nocapture",
				"--test-threads=1",
			])
			.env(FIXTURE_ENV, root)
			.env(NONCE_ENV, hex::encode(nonce))
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::inherit())
			.spawn()?;
		let (send, ready) = mpsc::channel();
		let mut worker = Self {
			child,
			input: None,
			job: Some(job),
			reader: None,
			ready,
			reaped: false,
		};
		// SAFETY: only this newly spawned child's borrowed process handle is assigned to the
		// live owned job. No START byte is sent until assignment succeeds; failure has no writes.
		let assigned = unsafe {
			AssignProcessToJobObject(
				worker.job.as_ref().expect("owned job").as_raw_handle(),
				worker.child.as_raw_handle(),
			)
		};
		if assigned == 0 {
			return Err(io::Error::last_os_error());
		}
		worker.input = worker.child.stdin.take();
		let output = worker
			.child
			.stdout
			.take()
			.ok_or_else(|| io::Error::other("missing worker stdout"))?;
		worker.reader = Some(thread::spawn(move || {
			let result = read_frame(output);
			let _ = send.send(result);
		}));
		let input = worker
			.input
			.as_mut()
			.ok_or_else(|| io::Error::other("missing worker stdin"))?;
		input.write_all(&[START_BYTE])?;
		input.flush()?;
		eprintln!("Create child started for {phase:?}; job assigned before START");
		Ok(worker)
	}

	pub(super) fn ready(&self) -> io::Result<ReadyFrame> {
		self.ready
			.recv_timeout(HANDSHAKE_TIMEOUT)
			.map_err(io::Error::other)?
	}

	pub(super) fn terminate(&mut self) -> io::Result<ExitStatus> {
		// Request explicit child termination, then always close its kill-on-close job
		// before any fallible wait. Neither the panic nor error path detaches the child.
		let killed = self.child.kill();
		drop(self.job.take());
		if let Err(error) = killed {
			// The assigned job is already closed. A failed explicit kill still requires the
			// bounded reap below: termination by last-job-handle closure can be asynchronous.
			eprintln!(
				"Explicit owned-child kill returned {error}; awaiting closed-job termination"
			);
		}
		let deadline = Instant::now() + REAP_TIMEOUT;
		let status = loop {
			if let Some(status) = self.child.try_wait()? {
				self.reaped = true;
				break status;
			}
			if Instant::now() >= deadline {
				return Err(io::Error::new(
					io::ErrorKind::TimedOut,
					"owned child did not terminate",
				));
			}
			thread::sleep(POLL);
		};
		drop(self.input.take());
		if let Some(reader) = self.reader.take() {
			let deadline = Instant::now() + JOIN_TIMEOUT;
			while !reader.is_finished() {
				if Instant::now() >= deadline {
					// The job is already closed. Abort this test binary instead of detaching a
					// blocked pipe reader after the owned child's writers should have closed.
					eprintln!("Owned child pipe-reader lifetime invariant failed");
					std::process::abort();
				}
				thread::sleep(POLL);
			}
			reader
				.join()
				.map_err(|_| io::Error::other("owned pipe reader panicked"))?;
		}
		Ok(status)
	}
}

impl Drop for Worker {
	fn drop(&mut self) {
		if (!self.reaped || self.reader.is_some()) && self.terminate().is_err() {
			// Job closure precedes abort: no owned subprocess or reader is detached.
			eprintln!("Owned child teardown failed after closing its kill-on-close job");
			std::process::abort();
		}
	}
}

fn create_job() -> io::Result<OwnedHandle> {
	// SAFETY: unnamed job, default token DACL, non-inherited handle, no privileges enabled.
	let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
	if raw.is_null() {
		return Err(io::Error::last_os_error());
	}
	// SAFETY: successful creation transferred one owned job handle.
	let job = unsafe { OwnedHandle::from_raw_handle(raw) };
	let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
	limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
	// SAFETY: exact SDK structure/class, live job. No breakaway or UI limits are enabled.
	let result = unsafe {
		SetInformationJobObject(
			job.as_raw_handle(),
			JobObjectExtendedLimitInformation,
			(&raw const limits).cast(),
			size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
		)
	};
	if result == 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(job)
}

fn read_frame(output: std::process::ChildStdout) -> io::Result<ReadyFrame> {
	let mut reader = io::BufReader::new(output.take(MAX_STDOUT_BYTES as u64));
	for _ in 0..MAX_STDOUT_LINES {
		let mut line = String::new();
		if reader.read_line(&mut line)? == 0 {
			break;
		}
		if let Some(offset) = line.find(FRAME_PREFIX) {
			let payload = line[offset + FRAME_PREFIX.len()..].trim_end();
			if payload.len() > MAX_FRAME_BYTES {
				return Err(io::Error::other("worker frame exceeds limit"));
			}
			return serde_json::from_str(payload).map_err(io::Error::other);
		}
	}
	Err(io::Error::other(
		"worker exited or exceeded its bounded output without a ready frame",
	))
}
