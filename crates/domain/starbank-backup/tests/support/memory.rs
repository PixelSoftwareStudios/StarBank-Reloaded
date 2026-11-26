use std::collections::BTreeMap;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use starbank_backup::{
	BackupId, BackupRecord, BackupStorage, BankSource, CommitState, ReadFailure, RecordFailure,
	Snapshot, StorageIntent,
};

#[derive(Debug, Default)]
pub(super) struct MemoryStorage {
	pub(super) files: BTreeMap<PathBuf, (u64, Vec<u8>)>,
	pub(super) writes: Vec<PathBuf>,
	pub(super) records: BTreeMap<BackupId, BackupRecord>,
	pub(super) record_failure: Option<RecordFailure<Error, &'static str>>,
	next_id: u64,
	pub(super) create_failure: Option<CreateFailure>,
	pub(super) replace_failure: Option<ReplaceFailure>,
}

#[derive(Debug)]
pub(super) enum CreateFailure {
	Rejected,
	PublishedButUncertain,
}

#[derive(Debug)]
pub(super) enum ReplaceFailure {
	ExternalChange,
	PublishedButUncertain,
}

impl MemoryStorage {
	pub(super) fn put(&mut self, path: &Path, bytes: &[u8]) -> u64 {
		self.next_id += 1;
		self.files
			.insert(path.to_owned(), (self.next_id, bytes.to_vec()));
		self.next_id
	}

	pub(super) fn bytes(&self, path: &Path) -> &[u8] {
		&self.files.get(path).expect("test file exists").1
	}

	pub(super) fn current(&self, source: &BankSource) -> u64 {
		self.files.get(source.path()).expect("test source exists").0
	}
}

impl BackupStorage for MemoryStorage {
	type Fingerprint = u64;
	type Receipt = u64;
	type Error = Error;
	type Issue = &'static str;

	fn read(
		&mut self,
		path: &Path,
		max_bytes: usize,
	) -> Result<Snapshot<Self::Fingerprint>, ReadFailure<Self::Error>> {
		let (fingerprint, bytes) = self.files.get(path).ok_or(ReadFailure::Missing)?;
		if bytes.len() > max_bytes {
			return Err(ReadFailure::Failed(ErrorKind::InvalidData.into()));
		}
		Ok(Snapshot {
			fingerprint: *fingerprint,
			bytes: bytes.clone(),
		})
	}

	fn commit(
		&mut self,
		path: &Path,
		bytes: &[u8],
		intent: StorageIntent<'_, Self::Fingerprint>,
		_max_bytes: usize,
	) -> CommitState<Self::Receipt, Self::Error, Self::Issue> {
		self.writes.push(path.to_owned());
		match intent {
			StorageIntent::CreateNew => {
				if self.files.contains_key(path) {
					return CommitState::AlreadyExists;
				}
				match self.create_failure.take() {
					Some(CreateFailure::Rejected) => {
						return CommitState::Rejected(ErrorKind::PermissionDenied.into());
					}
					Some(CreateFailure::PublishedButUncertain) => {
						self.put(path, bytes);
						return CommitState::Uncertain("create readback failed");
					}
					None => {}
				}
			}
			StorageIntent::ReplaceExpected(expected) => {
				let failure = self.replace_failure.take();
				if matches!(failure, Some(ReplaceFailure::ExternalChange)) {
					self.put(path, b"<External/>");
				}
				if self.files.get(path).map(|file| &file.0) != Some(expected) {
					return CommitState::Rejected(ErrorKind::AlreadyExists.into());
				}
				if matches!(failure, Some(ReplaceFailure::PublishedButUncertain)) {
					self.put(path, bytes);
					return CommitState::Uncertain("replace readback failed");
				}
			}
		}
		CommitState::Verified(self.put(path, bytes))
	}

	fn record_backup(
		&mut self,
		record: &BackupRecord,
	) -> Result<(), RecordFailure<Self::Error, Self::Issue>> {
		if let Some(failure) = self.record_failure.take() {
			return Err(failure);
		}
		let std::collections::btree_map::Entry::Vacant(entry) =
			self.records.entry(record.id().clone())
		else {
			return Err(RecordFailure::Rejected(ErrorKind::AlreadyExists.into()));
		};
		entry.insert(record.clone());
		Ok(())
	}
}
