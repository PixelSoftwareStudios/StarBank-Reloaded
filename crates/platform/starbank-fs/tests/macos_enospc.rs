#![cfg(target_os = "macos")]

use std::ffi::OsStr;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rustix::fs::{StatFs, XattrFlags, fstatfs, getxattr, setxattr};
use starbank_fs::{CommitIntent, CommitOutcome, StorageError, commit, read_snapshot};

#[path = "macos_enospc/calibration.rs"]
mod calibration;
#[path = "macos_enospc/fill_progress.rs"]
mod fill_progress;

use calibration::Calibration;

const TEST_FILESYSTEM: &str = "STARBANK_ENOSPC_APFS";
const FIXTURE_PARENT: &str = "/private/tmp";
const FIXTURE_PREFIX: &str = "starbank-apfs-enospc.";
const MOUNT_NAME: &str = "volume";
const MIB: usize = 1024 * 1024;
const MIN_CAPACITY: usize = 32 * MIB;
const MAX_CAPACITY: usize = 128 * MIB;
const FILL_TARGET_FREE: usize = 8 * MIB;
const PREPARED_SIZE: usize = 16 * MIB;
const FILL_CHUNK: usize = MIB;
const MAX_FILL_STEPS: usize = MAX_CAPACITY / FILL_CHUNK + 1;
const MAX_HEADROOM_ATTEMPTS: usize = 8;
const EXISTING: &str = "Existing.SC2Bank";
const NEW: &str = "New.SC2Bank";
const FILLER: &str = "space-control";
const RAW_CONTROL: &str = "native-write-control";
const ORIGINAL: &[u8] = br#"<Bank version="1"/>"#;
const RETRY: &[u8] = br#"<Bank version="2"/>"#;
const MODE: u32 = 0o640;
const PERMISSION_MASK: u32 = 0o7777;
const XATTR: &str = "com.starbank.enospc-test";
const XATTR_BYTES: &[u8] = b"synthetic original metadata";
const WRITE_PENDING: &str = "write pending file";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FillPolicy {
	ReportedThreshold,
	UntilEnospc,
}

struct FilledVolume {
	filled: usize,
	available: usize,
	calibrated: bool,
}

#[test]
#[ignore = "requires the disposable APFS image from verification/macos-enospc.sh"]
fn apfs_enospc_rejects_both_commit_intents_before_publication() -> io::Result<()> {
	let (root, volume, capacity) = fixture_io("validate disposable volume", fixture_volume())?;
	for policy in [FillPolicy::ReportedThreshold, FillPolicy::UntilEnospc] {
		eprintln!("APFS case {policy:?}: begin");
		if let Err(error) = exercise_commit_intents(&root, &volume, capacity, policy) {
			eprintln!("APFS case {policy:?}: failed: {error:?}");
			return Err(error);
		}
		eprintln!("APFS case {policy:?}: complete; test directory removed");
	}
	Ok(())
}

fn exercise_commit_intents(
	root: &Path,
	volume: &File,
	capacity: usize,
	policy: FillPolicy,
) -> io::Result<()> {
	let fixture = fixture_io("create test directory", tempfile::tempdir_in(root))?.keep();
	eprintln!(
		"APFS case {policy:?}: preserving case directory on failure: {}",
		fixture.display()
	);
	let existing = fixture.join(EXISTING);
	let target = fixture.join(NEW);
	let filler_path = fixture.join(FILLER);
	let control_path = fixture.join(RAW_CONTROL);
	fixture_io("write original bank", fs::write(&existing, ORIGINAL))?;
	fixture_io(
		"set original mode",
		fs::set_permissions(&existing, fs::Permissions::from_mode(MODE)),
	)?;
	fixture_io(
		"set original xattr",
		setxattr(&existing, XATTR, XATTR_BYTES, XattrFlags::empty()),
	)?;
	let before = read_snapshot(&existing, PREPARED_SIZE).expect("observe original bank");

	let mut control = fixture_io(
		"create native write control",
		create_exclusive(&control_path),
	)?;
	let mut calibration = Calibration::new(&fixture, capacity)?;
	let FilledVolume {
		filled,
		available,
		calibrated,
	} = fixture_io(
		"fill disposable volume",
		fill_volume(volume, &filler_path, policy),
	)?;
	if policy == FillPolicy::UntilEnospc {
		assert!(
			calibrated,
			"forced filler exhaustion must exercise calibration"
		);
	}
	eprintln!("APFS case {policy:?}: filler ready, calibrated={calibrated}");
	let prepared = vec![b'w'; PREPARED_SIZE];
	calibration.pack(&fixture, volume, &prepared, "before native control")?;
	let error = control
		.write_all(&prepared)
		.expect_err("native control must run out of APFS capacity");
	assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
	let control_written = fixture_io("inspect native write control", control.metadata())?.len();
	let control_blocks =
		fixture_io("inspect native control allocation", control.metadata())?.blocks();
	eprintln!(
		"APFS case {policy:?}: retained native control ENOSPC, length={control_written}, allocated blocks={control_blocks}, reported available={}",
		available_bytes(volume)?
	);

	for (destination, intent) in [
		(&target, CommitIntent::CreateNew),
		(
			&existing,
			CommitIntent::ReplaceExpected(before.fingerprint()),
		),
	] {
		let operation = match intent {
			CommitIntent::CreateNew => "create new",
			CommitIntent::ReplaceExpected(_) => "replace expected",
		};
		calibration.pack(&fixture, volume, &prepared, operation)?;
		calibration.release_metadata_reserve(&fixture, operation)?;
		eprintln!(
			"APFS case {policy:?}: {operation} reserve released; immediately before required commit, reported available {}",
			available_bytes(volume)?
		);
		eprintln!("APFS case {policy:?}: checking {operation} publication refusal");
		assert_staging_exhaustion(commit(destination, &prepared, intent, PREPARED_SIZE));
		let after = read_snapshot(&existing, PREPARED_SIZE).expect("reopen original bank");
		assert_eq!(after.fingerprint(), before.fingerprint());
		assert_eq!(after.bytes(), ORIGINAL);
		assert_eq!(
			fixture_io("verify original mode", fs::metadata(&existing))?.mode() & PERMISSION_MASK,
			MODE
		);
		let mut attribute = [0; XATTR_BYTES.len()];
		let size = fixture_io(
			"verify original xattr",
			getxattr(&existing, XATTR, &mut attribute),
		)?;
		assert_eq!(&attribute[..size], XATTR_BYTES);
		let names = fixture_io("enumerate pending-file cleanup", fs::read_dir(&fixture))?
			.map(|entry| entry.map(|entry| entry.file_name()))
			.collect::<io::Result<Vec<_>>>();
		let mut names = fixture_io("read pending-file cleanup entries", names)?;
		names.sort_unstable();
		let mut expected = calibration.names().to_vec();
		expected.extend([EXISTING.into(), FILLER.into(), RAW_CONTROL.into()]);
		expected.sort_unstable();
		assert_eq!(
			names, expected,
			"the failed create and both pending names must be absent",
		);
	}

	drop(control);
	fixture_io(
		"remove retained native control",
		fs::remove_file(control_path),
	)?;
	calibration.release(&fixture)?;
	fixture_io("remove filler", fs::remove_file(filler_path))?;
	let stats = fixture_io("measure reclaimed volume", fstatfs(volume))?;
	let recovered = bytes_from_blocks(&stats, stats.f_bavail)?;
	assert!(
		recovered > available,
		"removing the filler must reclaim APFS space"
	);
	eprintln!("APFS case {policy:?}: filler removed; checking successful retry");
	let followup = commit(&target, RETRY, CommitIntent::CreateNew, PREPARED_SIZE);
	assert!(
		matches!(followup, CommitOutcome::CommittedVerified(_)),
		"the retry must acquire the lease and publish after reclaiming space: {followup:?}",
	);
	assert_eq!(
		fixture_io("verify successful retry bytes", fs::read(target))?,
		RETRY
	);

	println!(
		"APFS case {policy:?}: capacity {capacity} bytes, filler {filled} bytes, available {available} bytes, calibrated={calibrated}, raw control stored {control_written} of {PREPARED_SIZE} bytes, both staging writes errno {}, recovered {recovered} bytes",
		libc::ENOSPC,
	);
	fixture_io(
		"remove completed case directory",
		fs::remove_dir_all(&fixture),
	)?;
	Ok(())
}

fn fixture_volume() -> io::Result<(PathBuf, File, usize)> {
	let root = std::env::var_os(TEST_FILESYSTEM)
		.map(PathBuf::from)
		.ok_or_else(|| {
			io::Error::other(format!(
				"run verification/macos-enospc.sh ({TEST_FILESYSTEM})"
			))
		})?;
	let parent = root
		.parent()
		.ok_or_else(|| io::Error::other("missing fixture parent directory"))?;
	assert!(
		root.file_name() == Some(OsStr::new(MOUNT_NAME))
			&& parent.parent() == Some(Path::new(FIXTURE_PARENT))
			&& parent
				.file_name()
				.and_then(OsStr::to_str)
				.and_then(|name| name.strip_prefix(FIXTURE_PREFIX))
				.is_some_and(|suffix| {
					!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
				})
			&& root.canonicalize()? == root,
		"expected an unsymlinked task-owned mount path from verification/macos-enospc.sh",
	);

	let volume = File::open(&root)?;
	let stats = fstatfs(&volume)?;
	let device = c_field_bytes(&stats.f_mntfromname);
	assert!(
		c_field_bytes(&stats.f_fstypename) == b"apfs"
			&& c_field_bytes(&stats.f_mntonname) == root.as_os_str().as_bytes()
			&& device.starts_with(b"/dev/disk")
			&& volume.metadata()?.dev() != parent.metadata()?.dev()
			&& stats.f_flags & libc::MNT_LOCAL as u32 != 0,
		"fixture must be its own mounted local APFS volume",
	);
	let capacity = bytes_from_blocks(&stats, stats.f_blocks)?;
	assert!(
		(MIN_CAPACITY..=MAX_CAPACITY).contains(&capacity),
		"fixture volume capacity {capacity} is outside the dedicated image limit",
	);
	// System-created dot directories can be present even on a newly formatted volume.
	for entry in root.read_dir()? {
		let name = entry?.file_name();
		assert!(
			name.as_bytes().starts_with(b"."),
			"fixture volume has existing data"
		);
	}
	Ok((root, volume, capacity))
}

fn c_field_bytes(field: &[libc::c_char]) -> Vec<u8> {
	field
		.iter()
		.take_while(|byte| **byte != 0)
		.map(|byte| byte.to_ne_bytes()[0])
		.collect()
}

fn bytes_from_blocks(stats: &StatFs, blocks: u64) -> io::Result<usize> {
	usize::try_from(blocks)
		.ok()
		.and_then(|blocks| blocks.checked_mul(stats.f_bsize as usize))
		.ok_or_else(|| io::Error::other("APFS size overflow"))
}

fn create_exclusive(path: &Path) -> io::Result<File> {
	OpenOptions::new().write(true).create_new(true).open(path)
}

fn fill_volume(volume: &File, path: &Path, policy: FillPolicy) -> io::Result<FilledVolume> {
	let mut file = fixture_io("create filler", create_exclusive(path))?;
	let chunk = vec![b'f'; FILL_CHUNK];
	let mut synced_bytes = 0;
	let mut last_count = 0;
	let mut available = available_bytes(volume)?;
	for observation in 1..=MAX_FILL_STEPS {
		let count = match fill_progress::next_step(policy, available, synced_bytes) {
			fill_progress::FillStep::Complete => {
				return Ok(FilledVolume {
					filled: synced_bytes,
					available,
					calibrated: false,
				});
			}
			fill_progress::FillStep::RestoreHeadroom => {
				return restore_headroom(&mut file, volume, synced_bytes, "reported free space");
			}
			fill_progress::FillStep::Write(count) => count,
			fill_progress::FillStep::RefuseBudget => {
				return refuse_fill_budget(&file, synced_bytes, available, last_count, observation);
			}
		};
		last_count = count;
		if let Err(error) = file.write_all(&chunk[..count]) {
			if error.raw_os_error() != Some(libc::ENOSPC) {
				return fixture_io("write filler", Err(error));
			}
			return restore_headroom(&mut file, volume, synced_bytes, "filler write ENOSPC");
		}
		if let Err(error) = file.sync_all() {
			if error.raw_os_error() != Some(libc::ENOSPC) {
				return fixture_io("sync filler", Err(error));
			}
			return restore_headroom(&mut file, volume, synced_bytes, "filler sync ENOSPC");
		}
		synced_bytes += count;
		available = available_bytes(volume)?;
	}
	refuse_fill_budget(&file, synced_bytes, available, last_count, MAX_FILL_STEPS)
}

fn refuse_fill_budget(
	file: &File,
	filled: usize,
	available: usize,
	last_count: usize,
	observations: usize,
) -> io::Result<FilledVolume> {
	let metadata = fixture_io("inspect refused filler progress", file.metadata())?;
	Err(io::Error::other(format!(
		"APFS filler refused bounded progress after {observations} observations: filled={filled}, current_available={available}, last_count={last_count}, file_length={}, allocated_blocks={}",
		metadata.len(),
		metadata.blocks(),
	)))
}

fn restore_headroom(
	file: &mut File,
	volume: &File,
	synced_bytes: usize,
	phase: &str,
) -> io::Result<FilledVolume> {
	if synced_bytes == 0 {
		return Err(io::Error::other(format!(
			"APFS {phase} left no synced filler bytes to reclaim"
		)));
	}
	// APFS free-block counters do not promise allocatable data plus metadata space.
	// Discard the unsynced tail and reclaim only this fixture's previously synced bytes.
	let mut retained = synced_bytes.saturating_sub(FILL_TARGET_FREE);
	for attempt in 0..MAX_HEADROOM_ATTEMPTS {
		eprintln!(
			"APFS headroom calibration after {phase}: attempt {}, synced filler {synced_bytes}, retaining {retained} bytes",
			attempt + 1
		);
		let reclaimed = file.set_len(retained as u64).and_then(|()| file.sync_all());
		match reclaimed {
			Ok(()) => {
				let actual = fixture_io("inspect calibrated filler", file.metadata())?.len();
				assert_eq!(
					actual, retained as u64,
					"only the owned filler was truncated"
				);
				let available = available_bytes(volume)?;
				eprintln!(
					"APFS calibrated filler {retained} bytes, reported available {available} bytes"
				);
				if available >= MIB {
					return Ok(FilledVolume {
						filled: retained,
						available,
						calibrated: true,
					});
				}
			}
			Err(error) if error.raw_os_error() == Some(libc::ENOSPC) => {
				eprintln!("APFS filler headroom sync/truncate still reports ENOSPC: {error:?}");
			}
			Err(error) => return fixture_io("restore filler headroom", Err(error)),
		}
		if retained == 0 {
			break;
		}
		retained = retained.saturating_sub(FILL_CHUNK);
	}
	Err(io::Error::other(format!(
		"APFS {phase} could not restore staging headroom within the bounded filler calibration"
	)))
}

fn available_bytes(volume: &File) -> io::Result<usize> {
	let stats = fixture_io("measure filler free blocks", fstatfs(volume))?;
	bytes_from_blocks(&stats, stats.f_bavail)
}

fn fixture_io<T, E: Into<io::Error>>(phase: &str, result: Result<T, E>) -> io::Result<T> {
	result.map_err(|error| {
		let error = error.into();
		eprintln!("APFS fixture I/O phase '{phase}' failed: {error:?}");
		error
	})
}

fn assert_staging_exhaustion(outcome: CommitOutcome) {
	let CommitOutcome::RejectedUnchanged(StorageError::Io { operation, source }) = outcome else {
		panic!("publication must be rejected on the exhausted image: {outcome:?}");
	};
	assert_eq!(operation, WRITE_PENDING);
	assert_eq!(source.raw_os_error(), Some(libc::ENOSPC));
}
