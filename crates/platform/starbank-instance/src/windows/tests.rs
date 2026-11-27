use std::{
	os::windows::ffi::OsStrExt as _,
	process,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, AtomicU32, Ordering},
		mpsc,
	},
	thread,
};

use tokio::{runtime::Builder, time::timeout};
use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED};

use super::*;
use crate::MAX_FRAME_LEN;

#[path = "tests/child.rs"]
mod child;
use child::QualificationChild;

const NAMESPACE_PREFIX: &str = "org.starbank.instance-test.";
const CHILD_TEST: &str = "windows::tests::run_qualification_child";
const CHILD_PARENT_PID: &str = "STARBANK_INSTANCE_TEST_PARENT_PID";
const CHILD_NAMESPACE: &str = "STARBANK_INSTANCE_TEST_NAMESPACE";
const CHILD_CASE: &str = "STARBANK_INSTANCE_TEST_CASE";
const TEST_BUDGET: Duration = Duration::from_secs(20);
const FIRST_BANK: &str = r"C:\StarBank fixture\星🪐.SC2Bank";
const OTHER_BANK: &str = r"C:\StarBank fixture\Other.SC2Bank";
const QUEUE_CAPACITY: usize = 2;
const UNPAIRED_HIGH_SURROGATE: u16 = 0xD800;

static NEXT_NAMESPACE: AtomicU32 = AtomicU32::new(0);

fn namespace() -> String {
	format!(
		"{NAMESPACE_PREFIX}{}-{}",
		process::id(),
		NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed)
	)
}

fn owner() -> security::LogonSid {
	security::LogonSid::current().expect("current Windows logon SID")
}

fn address(name: &str) -> security::Address {
	owner().endpoint(validate_namespace(name).expect("synthetic namespace"))
}

fn run(test: impl Future<Output = ()>) {
	Builder::new_current_thread()
		.enable_all()
		.build()
		.expect("Windows pipe test runtime")
		.block_on(async {
			timeout(TEST_BUDGET, test)
				.await
				.expect("pipe test deadline")
		});
}

fn path_request(paths: &[&str]) -> OpenRequest {
	OpenRequest::new(
		paths
			.iter()
			.map(|path| path.encode_utf16().collect())
			.collect(),
	)
	.expect("request")
}

fn focus_request() -> OpenRequest {
	path_request(&[])
}

fn unicode_request() -> OpenRequest {
	let mut odd: Vec<_> = r"C:\StarBank fixture\Unpaired-".encode_utf16().collect();
	odd.push(UNPAIRED_HIGH_SURROGATE);
	odd.extend(".sc2bank".encode_utf16());
	OpenRequest::new(vec![FIRST_BANK.encode_utf16().collect(), odd]).expect("Unicode request")
}

fn admission(queued: u8, duplicate: u8, refused: u8) -> QueueAdmission {
	QueueAdmission::new(queued, duplicate, refused).expect("admission counts")
}

fn path_units(paths: &[PathBuf]) -> Vec<Vec<u16>> {
	paths
		.iter()
		.map(|path| path.as_os_str().encode_wide().collect())
		.collect()
}

async fn primary(name: &str) -> Listener {
	match claim(name, &focus_request())
		.await
		.expect("initial pipe claim")
	{
		Claim::Primary(listener) => listener,
		Claim::Forwarded(result) => panic!("unexpected existing endpoint: {result:?}"),
	}
}

async fn forwarded(name: &str, request: &OpenRequest) -> QueueAdmission {
	match claim(name, request).await.expect("forwarding claim") {
		Claim::Forwarded(result) => result,
		Claim::Primary(_) => panic!("unexpected second primary"),
	}
}

async fn connect(address: &security::Address) -> NamedPipeClient {
	let deadline = Instant::now() + STARTUP_BUDGET;
	loop {
		match security::connect(address) {
			Ok(client) => return client,
			Err(error)
				if has_code(&error, ERROR_PIPE_BUSY) || has_code(&error, ERROR_FILE_NOT_FOUND) =>
			{
				let now = Instant::now();
				assert!(
					now < deadline,
					"waiting for the synthetic endpoint: {error}"
				);
				sleep_until((now + RETRY_INTERVAL).min(deadline)).await;
			}
			Err(error) => panic!("connect to synthetic endpoint: {error}"),
		}
	}
}

async fn disconnected(client: &mut NamedPipeClient) {
	let mut unexpected = [0];
	let closed = timeout(STARTUP_BUDGET, client.read(&mut unexpected))
		.await
		.expect("listener disconnected");
	match closed {
		Ok(0) => {}
		Err(error)
			if [ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED]
				.into_iter()
				.any(|code| has_code(&error, code)) => {}
		other => panic!("expected disconnection without an acknowledgement, got {other:?}"),
	}
}

async fn abort_listener(task: tokio::task::JoinHandle<Result<(), InstanceError>>) {
	task.abort();
	assert!(
		task.await
			.expect_err("listener was still running")
			.is_cancelled()
	);
}

#[test]
#[ignore = "private subprocess entry for isolated named-pipe qualification"]
fn run_qualification_child() {
	let Ok(parent) = std::env::var(CHILD_PARENT_PID) else {
		return;
	};
	let parent = parent.parse::<u32>().expect("synthetic parent PID");
	assert_ne!(parent, process::id());
	let name = std::env::var(CHILD_NAMESPACE).expect("synthetic namespace");
	let suffix = name
		.strip_prefix(&format!("{NAMESPACE_PREFIX}{parent}-"))
		.expect("parent-owned synthetic namespace");
	assert!(suffix.parse::<u32>().is_ok());
	let case = std::env::var(CHILD_CASE).expect("synthetic child case");
	run(async {
		match case.as_str() {
			"qualify-peer" => {
				let mut client = connect(&address(&name)).await;
				let owner = owner();
				security::qualification::verify_client_handle(&client, owner)
					.expect("client handle access and server identity");
				assert_eq!(
					owner.verify_server(&client).expect("server identity").pid(),
					parent
				);
				assert_eq!(
					forward(
						&mut client,
						&unicode_request(),
						Instant::now() + CONNECTION_BUDGET
					)
					.await
					.expect("admission after verified peer"),
					admission(2, 0, 0),
				);
			}
			"forward" => assert_eq!(
				forwarded(&name, &path_request(&[FIRST_BANK, OTHER_BANK])).await,
				admission(0, 1, 1),
			),
			"focus" => assert_eq!(forwarded(&name, &focus_request()).await, admission(0, 0, 0)),
			_ => panic!("unknown qualification child case"),
		}
	});
}

#[test]
#[ignore = "requires Windows kernel pipe ACL and same-logon subprocess qualification"]
fn qualify_windows_pipe_election_peers_and_admission() {
	run(async {
		let name = namespace();
		let mut listener = primary(&name).await;
		let owner = owner();
		let address = address(&name);
		security::qualification::verify_server_acl(&listener.server, owner)
			.expect("exact protected server DACL and single instance");
		security::qualification::verify_owner_cannot_change_acl(&address)
			.expect("same-user client cannot open WRITE_DAC");
		assert!(matches!(
			security::bind(&address, &owner),
			Err(error) if has_code(&error, ERROR_ACCESS_DENIED) || has_code(&error, ERROR_PIPE_BUSY)
		));

		let queue = Arc::new(Mutex::new(Vec::new()));
		let received = Arc::new(Mutex::new(Vec::new()));
		let pending = Arc::clone(&queue);
		let requests = Arc::clone(&received);
		let mut admit = move |paths: Vec<PathBuf>, deadline| {
			let mut pending = pending.lock().expect("synthetic admission queue");
			assert!(AdmissionDeadline::now() < deadline);
			let paths = path_units(&paths);
			requests.lock().expect("received paths").push(paths.clone());
			let (mut queued, mut duplicate, mut refused) = (0, 0, 0);
			for path in paths {
				if pending.contains(&path) {
					duplicate += 1;
				} else if pending.len() == QUEUE_CAPACITY {
					refused += 1;
				} else {
					pending.push(path);
					queued += 1;
				}
			}
			Ok(admission(queued, duplicate, refused))
		};

		let child = QualificationChild::launch(&name, "qualify-peer");
		listener
			.server
			.connect()
			.await
			.expect("accept child endpoint");
		assert_eq!(
			owner
				.verify_client(&listener.server)
				.expect("child identity")
				.pid(),
			child.pid()
		);
		timeout(
			CONNECTION_BUDGET,
			receive(
				&mut listener.server,
				&owner,
				&mut admit,
				Instant::now() + CONNECTION_BUDGET,
			),
		)
		.await
		.expect("qualified peer completion")
		.expect("admit real child request");
		listener.server.disconnect().expect("reuse primary server");
		child.verify_exit().await;

		let task = tokio::spawn(listener.serve(admit));
		for case in ["forward", "focus"] {
			QualificationChild::launch(&name, case).verify_exit().await;
		}
		assert_eq!(
			*queue.lock().expect("final queue"),
			unicode_request().paths()
		);
		assert_eq!(
			*received.lock().expect("forwarded requests"),
			[
				unicode_request().paths().to_vec(),
				path_request(&[FIRST_BANK, OTHER_BANK]).paths().to_vec(),
				Vec::new(),
			]
		);
		abort_listener(task).await;
		let _replacement = primary(&name).await;
	});
}

#[test]
#[ignore = "requires Windows kernel named pipes"]
fn qualify_bad_and_stalled_clients_release_listener() {
	run(async {
		let name = namespace();
		let address = address(&name);
		let listener = primary(&name).await;
		let expected = unicode_request();
		let encoded = expected.encode();
		let admissions = Arc::new(Mutex::new(Vec::new()));
		let observed = Arc::clone(&admissions);
		let task = tokio::spawn(listener.serve(move |paths, _| {
			let count = u8::try_from(paths.len()).expect("bounded paths");
			observed
				.lock()
				.expect("admission log")
				.push(path_units(&paths));
			Ok(admission(count, 0, 0))
		}));

		let malformed = [0; HEADER_LEN];
		let mut oversized = focus_request().encode();
		let length_offset = HEADER_LEN - size_of::<u32>();
		oversized[length_offset..].copy_from_slice(
			&u32::try_from(MAX_FRAME_LEN)
				.expect("bounded frame size")
				.to_le_bytes(),
		);
		let rejected_path = path_request(&["Relative.SC2Bank"]).encode();
		for invalid in [malformed.as_slice(), &oversized, &rejected_path] {
			let mut client = connect(&address).await;
			client
				.write_all(invalid)
				.await
				.expect("send invalid test frame");
			disconnected(&mut client).await;
		}
		{
			let mut client = connect(&address).await;
			client
				.write_all(&encoded[..HEADER_LEN - 1])
				.await
				.expect("send a partial header");
		}
		let mut stalled = connect(&address).await;
		stalled
			.write_all(&encoded[..HEADER_LEN + 1])
			.await
			.expect("send header and retain a stalled body");
		assert_eq!(forwarded(&name, &expected).await, admission(2, 0, 0));
		disconnected(&mut stalled).await;
		assert_eq!(
			*admissions.lock().expect("admission log"),
			[expected.paths().to_vec()]
		);
		abort_listener(task).await;
	});
}

#[test]
#[ignore = "requires Windows kernel named pipes"]
fn qualify_ack_loss_does_not_retry_delivery() {
	run(async {
		let name = namespace();
		let request = unicode_request();
		let encoded = request.encode();
		let owner = owner();
		let mut server = security::bind(&address(&name), &owner).expect("synthetic receiver");
		let recorder = tokio::spawn(async move {
			server
				.connect()
				.await
				.expect("accept one forwarding client");
			let _peer = owner.verify_client(&server).expect("same-logon client");
			let mut recorded = vec![0; encoded.len()];
			server
				.read_exact(&mut recorded)
				.await
				.expect("record accepted request bytes");
			assert_eq!(recorded, encoded);
			// Drop the real receiver without writing its acknowledgement.
		});
		assert!(matches!(
			claim(&name, &request).await,
			Err(InstanceError::DeliveryUncertain { .. }),
		));
		recorder
			.await
			.expect("receiver admitted exactly one request");
		let _replacement = primary(&name).await;
	});
}

#[test]
#[ignore = "requires Windows kernel named pipes"]
fn qualify_cancel_pending_read_and_reacquire_endpoint() {
	run(async {
		let name = namespace();
		let listener = primary(&name).await;
		let guard = listener
			.retain_endpoint()
			.expect("duplicate primary endpoint");
		let connected = Arc::new(AtomicBool::new(false));
		let ready = Arc::clone(&connected);
		let task = tokio::spawn(async move {
			let Listener { mut server, owner } = listener;
			server.connect().await.expect("accept a client");
			let _peer = owner.verify_client(&server).expect("same-logon client");
			ready.store(true, Ordering::Release);
			let mut header = [0; HEADER_LEN];
			server
				.read_exact(&mut header)
				.await
				.expect("read should be cancelled");
		});
		let mut client = connect(&address(&name)).await;
		client
			.write_all(&focus_request().encode()[..HEADER_LEN - 1])
			.await
			.expect("incomplete header");
		while !connected.load(Ordering::Acquire) {
			sleep_until(Instant::now() + RETRY_INTERVAL).await;
		}
		task.abort();
		assert!(
			task.await
				.expect_err("pending native read cancelled")
				.is_cancelled()
		);
		assert!(matches!(
			security::bind(&address(&name), &owner()),
			Err(error) if has_code(&error, ERROR_ACCESS_DENIED) || has_code(&error, ERROR_PIPE_BUSY)
		));
		drop(guard);
		disconnected(&mut client).await;
		drop(client);
		let _replacement = primary(&name).await;
	});
}

#[test]
#[ignore = "requires Windows kernel named pipes"]
fn qualify_admission_deadline_is_checked_under_queue_lock() {
	run(async {
		let name = namespace();
		let listener = primary(&name).await;
		let queue = Arc::new(Mutex::new(Vec::new()));
		let pending = Arc::clone(&queue);
		let (locked, started) = mpsc::channel();
		let (admitting, deadline) = mpsc::channel::<AdmissionDeadline>();
		let holder = thread::spawn(move || {
			let _guard = pending.lock().expect("hold synthetic queue");
			locked.send(()).expect("queue lock acquired");
			let cutoff = deadline
				.recv_timeout(STARTUP_BUDGET)
				.expect("callback reached admission");
			thread::sleep(
				cutoff.saturating_duration_since(AdmissionDeadline::now()) + RETRY_INTERVAL,
			);
		});
		started
			.recv_timeout(STARTUP_BUDGET)
			.expect("held queue lock");

		let pending = Arc::clone(&queue);
		let task = tokio::spawn(listener.serve(move |paths, deadline| {
			admitting
				.send(deadline)
				.expect("deliver actual connection deadline");
			let mut queue = pending.lock().expect("acquire queue only after deadline");
			let count = u8::try_from(paths.len()).expect("bounded paths");
			if AdmissionDeadline::now() >= deadline {
				Ok(admission(0, 0, count))
			} else {
				queue.extend(paths);
				Ok(admission(count, 0, 0))
			}
		}));
		assert!(matches!(
			claim(&name, &unicode_request()).await,
			Err(InstanceError::DeliveryUncertain { .. }),
		));
		holder.join().expect("queue lock released");
		assert!(queue.lock().expect("synthetic queue").is_empty());
		abort_listener(task).await;
	});
}
