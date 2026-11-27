//! Process-death evidence for the real candidate while Windows remains running.
use super::super::{
	Identity, Observation,
	rename_qualification::{prepare_reduced_fixture, with_reduced_token},
	stage_security::PrivateStageSecurity,
};
use super::{
	CommitPhase, Scope, commit_candidate,
	lease::{Contention, keep_existing_alive},
	try_create_scoped,
};
use crate::{CommitOutcome, StorageError};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Read as _, Write as _};
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_ARCHIVE;

mod process_io;
use process_io::Worker;

const NONCE_BYTES: usize = 16;
type Nonce = [u8; NONCE_BYTES];

const FIXTURE_ENV: &str = "STARBANK_CREATE_PROCESS_FIXTURE";
const NONCE_ENV: &str = "STARBANK_CREATE_PROCESS_NONCE";
const FIXTURE_PREFIX: &str = "starbank-create-process-";
const WORKER_TEST: &str = "windows::create::process_tests::windows_create_process_worker";
const MARKER: &str = "SyntheticFixture.json";
const TARGET: &str = "ProcessBank.SC2Bank";
const PAYLOAD: &[u8] = b"<Bank><!-- synthetic child process transaction --></Bank>";
const MAX_BYTES: usize = 1024;
const MAX_FRAME_BYTES: usize = 1024;
const MAX_STDOUT_BYTES: usize = 4096;
const MAX_STDOUT_LINES: usize = 8;
const FRAME_PREFIX: &str = "STARBANK_CREATE_READY ";
const START_BYTE: u8 = b'S';

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
	Staged,
	Published,
	DirectorySynced,
}
impl Phase {
	fn matches(self, phase: CommitPhase) -> bool {
		matches!(
			(self, phase),
			(Self::Staged, CommitPhase::Staged)
				| (Self::Published, CommitPhase::Published)
				| (Self::DirectorySynced, CommitPhase::DirectorySynced)
		)
	}
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileId {
	volume: u64,
	id: [u8; 16],
}
impl From<Identity> for FileId {
	fn from(identity: Identity) -> Self {
		Self {
			volume: identity.volume,
			id: identity.file_id,
		}
	}
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MarkerKind {
	StarbankSyntheticCreateV1,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Authorization {
	kind: MarkerKind,
	nonce: Nonce,
	phase: Phase,
	parent: FileId,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadyFrame {
	nonce: Nonce,
	phase: Phase,
	parent: FileId,
	file: FileId,
	len: u64,
	sha256: [u8; 32],
}
#[derive(Debug, PartialEq, Eq)]
struct Evidence {
	names: Vec<OsString>,
	file: FileId,
	bytes: Vec<u8>,
	sha256: [u8; 32],
	attributes: u32,
}

pub(super) fn qualify_process_cuts(fixture_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
	for phase in [Phase::Staged, Phase::Published, Phase::DirectorySynced] {
		let temporary = tempfile::Builder::new()
			.prefix(FIXTURE_PREFIX)
			.tempdir_in(fixture_root)?;
		let root = prepare_reduced_fixture(&temporary)?;
		let scope = Scope::open(&root.join(TARGET))?;
		let mut nonce = [0; NONCE_BYTES];
		getrandom::fill(&mut nonce).map_err(|error| io::Error::other(error.to_string()))?;
		let authorization = Authorization {
			kind: MarkerKind::StarbankSyntheticCreateV1,
			nonce,
			phase,
			parent: scope.directory.identity.into(),
		};
		let bytes = serde_json::to_vec(&authorization)?;
		assert!(bytes.len() <= MAX_FRAME_BYTES);
		let mut marker = OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(root.join(MARKER))?;
		marker.write_all(&bytes)?;
		marker.sync_all()?;
		drop(marker);
		let mut worker = Worker::spawn(&root, nonce, phase)?;
		let frame = worker.ready()?;
		assert_eq!(frame.nonce, nonce);
		assert_eq!(frame.phase, phase);
		assert_eq!(frame.parent, FileId::from(scope.directory.identity));
		let before = evidence(&scope, phase)?;
		assert_eq!(frame.file, before.file);
		assert_eq!(frame.len, PAYLOAD.len() as u64);
		assert_eq!(frame.sha256, before.sha256);
		// Opening an existing mutex never consumes abandoned state or grants ownership.
		let keeper = keep_existing_alive(scope.directory.identity)?;
		assert_parent_contention(&root.join(TARGET), Contention::Busy)?;
		assert_eq!(
			evidence(&scope, phase)?,
			before,
			"busy attempt must introduce no stage or mutation"
		);
		let status = worker.terminate()?;
		assert!(
			!status.success(),
			"worker must be killed at its blocked transaction checkpoint"
		);
		let after = evidence(&scope, phase)?;
		assert_eq!(
			after, before,
			"process death must retain the exact private artifact"
		);
		assert_parent_contention(&root.join(TARGET), Contention::Abandoned)?;
		assert_eq!(
			evidence(&scope, phase)?,
			before,
			"abandoned attempt must not clean or replay old artifacts"
		);
		drop(keeper);
		eprintln!(
			"Candidate process cut {phase:?}: live Busy, killed/reaped, retained ID/digest/security, observed abandoned rejection; no power-loss proof"
		);
	}
	Ok(())
}

fn assert_contention(outcome: CommitOutcome, expected: Contention) {
	let CommitOutcome::RejectedUnchanged(StorageError::Io { source, .. }) = outcome else {
		panic!("expected lease refusal: {outcome:?}");
	};
	assert_eq!(source.kind(), io::ErrorKind::ResourceBusy);
	assert_eq!(
		source
			.get_ref()
			.and_then(|error| error.downcast_ref::<Contention>()),
		Some(&expected)
	);
}

fn names(scope: &Scope) -> Result<Vec<OsString>, StorageError> {
	let mut names = cap_primitives::fs::read_base_dir(&scope.directory.parent)
		.map_err(|error| StorageError::io("list process fixture", error))?
		.map(|entry| entry.map(|entry| entry.file_name()))
		.collect::<io::Result<Vec<_>>>()
		.map_err(|error| StorageError::io("read process fixture entry", error))?;
	names.sort_unstable();
	Ok(names)
}

fn evidence(scope: &Scope, phase: Phase) -> Result<Evidence, StorageError> {
	let names = names(scope)?;
	if names.len() != 2 || !names.contains(&OsString::from(MARKER)) {
		return Err(StorageError::Conflict);
	}
	let name = names
		.iter()
		.find(|name| name.as_os_str() != MARKER)
		.ok_or(StorageError::Conflict)?;
	if (phase == Phase::Staged) == (name.as_os_str() == TARGET) {
		return Err(StorageError::Conflict);
	}
	let (file, snapshot) = scope.snapshot(name, MAX_BYTES)?;
	PrivateStageSecurity::current()?.verify(&file)?;
	let observation = Observation::read(&file)?;
	if observation.attributes != FILE_ATTRIBUTE_ARCHIVE || snapshot.bytes() != PAYLOAD {
		return Err(StorageError::Conflict);
	}
	scope.ensure_bound()?;
	Ok(Evidence {
		names,
		file: observation.identity.into(),
		bytes: snapshot.bytes().to_vec(),
		sha256: *snapshot.fingerprint().sha256(),
		attributes: observation.attributes,
	})
}

#[test]
#[ignore = "internal child entry: requires the parent-created nonce, marker, job and stdin gate"]
fn windows_create_process_worker() -> Result<(), Box<dyn std::error::Error>> {
	let root = std::env::var_os(FIXTURE_ENV)
		.map(std::path::PathBuf::from)
		.ok_or_else(|| io::Error::other("worker requires an owned fixture"))?;
	if !root
		.file_name()
		.and_then(|name| name.to_str())
		.is_some_and(|name| name.starts_with(FIXTURE_PREFIX))
	{
		return Err(io::Error::other("worker fixture prefix is invalid").into());
	}
	let nonce_text = std::env::var(NONCE_ENV).map_err(io::Error::other)?;
	if nonce_text.len() != NONCE_BYTES * 2 {
		return Err(io::Error::other("worker nonce length is invalid").into());
	}
	let mut nonce = [0; NONCE_BYTES];
	hex::decode_to_slice(nonce_text, &mut nonce)
		.map_err(|error| io::Error::other(error.to_string()))?;
	let mut start = [0];
	io::stdin().lock().read_exact(&mut start)?;
	if start != [START_BYTE] {
		return Err(io::Error::other("worker START gate is invalid").into());
	}
	with_reduced_token(move || {
		let scope = Scope::open(&root.join(TARGET))?;
		if root.canonicalize()? != root {
			return Err(io::Error::other("worker fixture must be canonical").into());
		}
		let (_, marker) = scope.snapshot(std::ffi::OsStr::new(MARKER), MAX_FRAME_BYTES)?;
		let authorization: Authorization = serde_json::from_slice(marker.bytes())?;
		if authorization.nonce != nonce
			|| authorization.parent != FileId::from(scope.directory.identity)
			|| names(&scope)? != [OsString::from(MARKER)]
		{
			return Err(io::Error::other("worker fixture authorization failed").into());
		}
		let phase = authorization.phase;
		let outcome = try_create_scoped(&scope, PAYLOAD, MAX_BYTES, |checkpoint| {
			if !phase.matches(checkpoint) {
				return Ok(());
			}
			let observed = evidence(&scope, phase)?;
			let frame = ReadyFrame {
				nonce,
				phase,
				parent: scope.directory.identity.into(),
				file: observed.file,
				len: observed.bytes.len() as u64,
				sha256: observed.sha256,
			};
			let frame = serde_json::to_string(&frame).map_err(|error| {
				StorageError::io("encode worker frame", io::Error::other(error))
			})?;
			if frame.len() > MAX_FRAME_BYTES {
				return Err(StorageError::io(
					"encode worker frame",
					io::Error::other("oversized ready frame"),
				));
			}
			let mut output = io::stdout().lock();
			writeln!(output, "{FRAME_PREFIX}{frame}")
				.and_then(|()| output.flush())
				.map_err(|error| StorageError::io("send worker frame", error))?;
			drop(output);
			// Keep the lease and restrictive writer alive until the parent kills this process.
			// Parent retains stdin until reaping; EOF/error therefore cannot simulate death.
			let mut unexpected = [0];
			io::stdin()
				.lock()
				.read_exact(&mut unexpected)
				.map_err(|error| StorageError::io("wait at process checkpoint", error))?;
			Err(StorageError::io(
				"wait at process checkpoint",
				io::Error::other("worker unexpectedly resumed"),
			))
		})?;
		let error = match outcome {
			CommitOutcome::RejectedUnchanged(error) => {
				io::Error::other(format!("worker ended before checkpoint: {error}"))
			}
			CommitOutcome::CommitUncertain(issue) => {
				io::Error::other(format!("worker ended before checkpoint: {}", issue.error()))
			}
			CommitOutcome::CommittedVerified(_) => {
				io::Error::other("worker committed without stopping at its authorized checkpoint")
			}
		};
		Err(error.into())
	})?;
	Ok(())
}

fn assert_parent_contention(path: &Path, expected: Contention) -> io::Result<()> {
	let path = path.to_path_buf();
	with_reduced_token(move || {
		assert_contention(commit_candidate(&path, PAYLOAD, MAX_BYTES), expected);
		Ok(())
	})
}
