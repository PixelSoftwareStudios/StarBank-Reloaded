use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;

use super::{
	CommitIntent, CommitOutcome, File, MIB, PREPARED_SIZE, StorageError, WRITE_PENDING,
	available_bytes, commit, create_exclusive, fixture_io, read_snapshot,
};

const CALIBRATION_SIZES: [usize; 5] = [PREPARED_SIZE, 8 * MIB, 4 * MIB, 2 * MIB, MIB];
const SETTLE_PASSES: usize = 2;
const SETTLE_DELAY: Duration = Duration::from_millis(250);
const CALIBRATION_ROUNDS: usize = 3;
const REJECTIONS_PER_ROUND: usize = CALIBRATION_SIZES.len() + SETTLE_PASSES;
const MAX_REQUESTED_CAPACITY_MULTIPLE: usize = 8;
const METADATA_RESERVE_SIZE: usize = 256 * 1024;
const METADATA_RESERVES: [&str; 2] = ["metadata-create-reserve", "metadata-replace-reserve"];

pub(super) struct Calibration {
	names: Vec<OsString>,
	capacity: usize,
	successful_bytes: usize,
	attempts: usize,
	requested_bytes: usize,
	released_reserves: usize,
}

impl Calibration {
	pub(super) fn new(root: &Path, capacity: usize) -> io::Result<Self> {
		let mut names = Vec::new();
		for name in METADATA_RESERVES {
			let mut file = fixture_io(
				"create owned metadata reserve",
				create_exclusive(&root.join(name)),
			)?;
			fixture_io(
				"write metadata reserve",
				file.write_all(&vec![b'm'; METADATA_RESERVE_SIZE]),
			)?;
			fixture_io("sync metadata reserve", file.sync_all())?;
			let metadata = fixture_io("inspect metadata reserve", file.metadata())?;
			eprintln!(
				"APFS metadata reserve {name}: length {}, allocated blocks {}",
				metadata.len(),
				metadata.blocks()
			);
			names.push(OsString::from(name));
		}
		Ok(Self {
			names,
			capacity,
			successful_bytes: 0,
			attempts: 0,
			requested_bytes: 0,
			released_reserves: 0,
		})
	}

	pub(super) fn release_metadata_reserve(&mut self, root: &Path, phase: &str) -> io::Result<()> {
		let name = METADATA_RESERVES[self.released_reserves];
		fixture_io(
			"release owned stage-metadata reserve",
			fs::remove_file(root.join(name)),
		)?;
		self.released_reserves += 1;
		self.names.retain(|retained| retained != name);
		eprintln!("APFS calibration {phase}: released only owned metadata reserve {name}");
		Ok(())
	}

	pub(super) fn names(&self) -> &[OsString] {
		&self.names
	}

	pub(super) fn pack(
		&mut self,
		root: &Path,
		volume: &File,
		prepared: &[u8],
		phase: &str,
	) -> io::Result<()> {
		for size in CALIBRATION_SIZES {
			self.pack_size(root, volume, &prepared[..size], phase)?;
		}
		// APFS may release earlier transaction reservations after a rejected stage.
		// These are bounded observations, never authority that the next write must fail.
		for _ in 0..SETTLE_PASSES {
			std::thread::sleep(SETTLE_DELAY);
			self.pack_size(root, volume, &prepared[..MIB], phase)?;
		}
		Ok(())
	}

	fn pack_size(
		&mut self,
		root: &Path,
		volume: &File,
		bytes: &[u8],
		phase: &str,
	) -> io::Result<()> {
		loop {
			self.attempts += 1;
			self.requested_bytes += bytes.len();
			if self.attempts > self.capacity / MIB + CALIBRATION_ROUNDS * REJECTIONS_PER_ROUND
				|| self.requested_bytes > self.capacity * MAX_REQUESTED_CAPACITY_MULTIPLE
			{
				return Err(io::Error::other(format!(
					"APFS calibration {phase} exhausted its bounded allocation budget"
				)));
			}
			let name = OsString::from(format!("calibration-{:03}.SC2Bank", self.attempts));
			let path = root.join(&name);
			let outcome = commit(&path, bytes, CommitIntent::CreateNew, PREPARED_SIZE);
			match outcome {
				CommitOutcome::CommittedVerified(receipt) => {
					let snapshot = read_snapshot(&path, PREPARED_SIZE)
						.expect("independently observe retained calibration output");
					assert_eq!(snapshot.fingerprint(), receipt.fingerprint());
					assert_eq!(snapshot.bytes(), bytes);
					assert!(receipt.recovery().is_none());
					let metadata = fixture_io("inspect verified calibration", fs::metadata(&path))?;
					self.successful_bytes += bytes.len();
					assert!(self.successful_bytes <= self.capacity);
					eprintln!(
						"APFS calibration {phase}: verified attempt {}, requested {}, length {}, allocated blocks {}, reported available {}, receipt {receipt:?}",
						self.attempts,
						bytes.len(),
						metadata.len(),
						metadata.blocks(),
						available_bytes(volume)?,
					);
					self.names.push(name);
				}
				CommitOutcome::RejectedUnchanged(StorageError::Io { operation, source })
					if operation == WRITE_PENDING
						&& source.raw_os_error() == Some(libc::ENOSPC) =>
				{
					eprintln!(
						"APFS calibration {phase}: attempt {} requested {}, actual write-pending ENOSPC, retained verified bytes {}, reported available {}",
						self.attempts,
						bytes.len(),
						self.successful_bytes,
						available_bytes(volume)?,
					);
					assert!(!path.try_exists()?, "rejected calibration must not publish");
					return Ok(());
				}
				outcome => {
					return Err(io::Error::other(format!(
						"APFS calibration {phase}, attempt {}, size {} did not reach write-pending ENOSPC: {outcome:?}",
						self.attempts,
						bytes.len(),
					)));
				}
			}
		}
	}

	pub(super) fn release(&self, root: &Path) -> io::Result<()> {
		for name in &self.names {
			fixture_io(
				"remove verified calibration after both refusals",
				fs::remove_file(root.join(name)),
			)?;
		}
		Ok(())
	}
}
