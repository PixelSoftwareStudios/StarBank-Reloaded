use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroUsize;

use crate::{BackupId, BackupRecord, BankSource};

const MAX_CATALOG_RECORDS: usize = 10_000;

/// Last observation of a cataloged backup's availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupHealth {
	/// The snapshot was verified in storage.
	Verified,
	/// Storage established that the snapshot was missing.
	Missing,
	/// Contents are unreadable, mismatched, or not yet checked in this session.
	Unverified,
}

/// Summary of a bank's visible backup history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupStatus {
	/// No snapshot is cataloged for this bank.
	NeverBackedUp,
	/// Availability of the most recent cataloged snapshot.
	Latest(BackupHealth),
}

/// The selection policy for an explicit retention review.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionPolicy {
	/// Keep all snapshots.
	KeepAll,
	/// List older verified snapshots beyond a minimum keep count.
	KeepNewest(NonZeroUsize),
}

/// Validated backup metadata plus session-scoped first-overwrite state.
///
/// The caller owns persistent record storage. Importing metadata does not give its
/// snapshot verified status. Instantiate a new catalog to start a new edit session.
#[derive(Debug)]
pub struct BackupCatalog {
	records: BTreeMap<BackupId, Entry>,
	pub(crate) protected_overwrites: BTreeSet<BankSource>,
	pub(crate) max_bytes: usize,
}

#[derive(Debug)]
struct Entry {
	record: BackupRecord,
	health: BackupHealth,
}

impl BackupCatalog {
	/// Creates an empty catalog with a strict per-bank snapshot read and write limit.
	#[must_use]
	pub fn new(max_bytes: NonZeroUsize) -> Self {
		Self {
			records: BTreeMap::new(),
			protected_overwrites: BTreeSet::new(),
			max_bytes: max_bytes.get(),
		}
	}

	/// Imports a previously recorded snapshot without trusting its storage state.
	///
	/// # Errors
	///
	/// Rejects duplicate IDs and entries beyond the configured limits.
	pub fn import(&mut self, record: BackupRecord) -> Result<(), CatalogError> {
		self.ensure_capacity()?;
		if record.integrity().byte_length > self.max_bytes {
			return Err(CatalogError::SnapshotTooLarge);
		}
		let std::collections::btree_map::Entry::Vacant(entry) =
			self.records.entry(record.id().clone())
		else {
			return Err(CatalogError::DuplicateId);
		};
		entry.insert(Entry {
			record,
			health: BackupHealth::Unverified,
		});
		Ok(())
	}

	/// Returns all catalog metadata, including missing and unverified snapshots.
	pub fn entries(&self) -> impl Iterator<Item = (&BackupRecord, BackupHealth)> {
		self.records
			.values()
			.map(|entry| (&entry.record, entry.health))
	}

	/// Returns the snapshot record and last observed health, if cataloged.
	#[must_use]
	pub fn get(&self, id: &BackupId) -> Option<(&BackupRecord, BackupHealth)> {
		self.records
			.get(id)
			.map(|entry| (&entry.record, entry.health))
	}

	/// Lists snapshots for the exact bank, newest timestamp first.
	#[must_use]
	pub fn history(&self, source: &BankSource) -> Vec<(&BackupRecord, BackupHealth)> {
		let mut entries: Vec<_> = self
			.entries()
			.filter(|(record, _)| record.source() == source)
			.collect();
		entries.sort_unstable_by_key(|(record, _)| {
			std::cmp::Reverse((record.timestamp(), record.sequence(), record.id()))
		});
		entries
	}

	/// Returns a summary without reading any bank or snapshot.
	#[must_use]
	pub fn status(&self, source: &BankSource) -> BackupStatus {
		let latest = self
			.entries()
			.filter(|(record, _)| record.source() == source)
			.max_by_key(|(record, _)| (record.timestamp(), record.sequence(), record.id()));
		latest.map_or(BackupStatus::NeverBackedUp, |(_, health)| {
			BackupStatus::Latest(health)
		})
	}

	/// Lists older verified records eligible for a separate user-approved retention action.
	///
	/// No bytes are removed or changed. Missing and unverified records stay visible.
	#[must_use]
	pub fn retention_candidates(
		&self,
		source: &BankSource,
		policy: RetentionPolicy,
	) -> Vec<&BackupRecord> {
		let RetentionPolicy::KeepNewest(keep) = policy else {
			return Vec::new();
		};
		self.history(source)
			.into_iter()
			.filter_map(|(record, health)| (health == BackupHealth::Verified).then_some(record))
			.skip(keep.get())
			.collect()
	}

	pub(crate) fn ensure_capacity(&self) -> Result<(), CatalogError> {
		if self.records.len() == MAX_CATALOG_RECORDS {
			return Err(CatalogError::Capacity);
		}
		Ok(())
	}

	pub(crate) fn insert_verified(&mut self, record: BackupRecord) -> BackupId {
		let id = record.id().clone();
		self.records.insert(
			id.clone(),
			Entry {
				record,
				health: BackupHealth::Verified,
			},
		);
		id
	}

	pub(crate) fn set_health(&mut self, id: &BackupId, health: BackupHealth) {
		if let Some(entry) = self.records.get_mut(id) {
			entry.health = health;
		}
	}
}

/// Rejected backup catalog metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CatalogError {
	/// The record limit for this catalog is exhausted.
	Capacity,
	/// A record already refers to this backup location.
	DuplicateId,
	/// Snapshot metadata exceeds this catalog's snapshot limit.
	SnapshotTooLarge,
}

impl Display for CatalogError {
	fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Capacity => "backup catalog record limit reached",
			Self::DuplicateId => "backup ID already exists in the catalog",
			Self::SnapshotTooLarge => "backup snapshot exceeds the byte limit",
		})
	}
}

impl Error for CatalogError {}
