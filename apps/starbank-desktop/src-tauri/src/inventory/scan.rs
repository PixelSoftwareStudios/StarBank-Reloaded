use std::{
	collections::BTreeMap,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use starbank_map_index::{
	Cancellation, IndexLimits, IndexRequest, PreparedCacheUpdate, ScanError,
	prepare_map_cache_rebuild, prepare_map_cache_refresh,
};
use starbank_paths::{
	PathError, PathSelection, ScanLimits, get_sc2_root_path, inventory_banks, is_valid_bank_path,
};

use super::{
	InventoryError, InventoryProgress, InventoryReason, InventorySnapshot, InventoryStatus,
	RootChange, RootRecordStatus, ScanLease,
	authority::{AuthorizedRoot, DirectoryIdentity, is_native_disk_path, ordinary_directories},
	blocked_saved_root,
	progress::InventoryProgressPublisher,
	projection::{self, OpaqueIds, Projection, project},
	root_record::ManualRoot,
};

const BANK_SCAN_DEPTH: usize = 32;
const BANK_SCAN_ENTRIES: NonZeroUsize = NonZeroUsize::new(24_000).unwrap();
const BANK_SCAN_LIMITS: ScanLimits = ScanLimits::new(BANK_SCAN_DEPTH, BANK_SCAN_ENTRIES);
const MAP_SCAN_ENTRIES: usize = 65_536;
const MAP_SCAN_ARCHIVES: usize = 2_048;

#[derive(Clone, Copy)]
pub(super) enum MapCache<'a> {
	Reuse(Option<&'a Path>),
	Rebuild(&'a Path),
}

#[derive(Clone, Copy)]
pub(super) struct MapScanOptions<'a> {
	pub(super) cache: MapCache<'a>,
	pub(super) max_workers: Option<NonZeroUsize>,
}

pub(super) type CollectedInventory = (Projection, Option<ManualRoot>, Option<PreparedCacheUpdate>);

impl ScanLease<'_> {
	pub(super) fn publish(
		self,
		cache: MapCache<'_>,
		((mut snapshot, banks), accepted_root, cache_update): CollectedInventory,
	) -> Result<InventorySnapshot, InventoryError> {
		let recovering = self.root_record_status == RootRecordStatus::Unreadable;
		let mut state = self
			.service
			.state
			.lock()
			.map_err(|_| InventoryError::Unavailable)?;
		check_cancelled(&self.cancellation)?;
		// Use the same lock as cancellation for both kinds of cache publication.
		match cache {
			MapCache::Rebuild(_) if snapshot.status == InventoryStatus::Ready => {
				cache_update
					.filter(|_| !snapshot.incomplete)
					.ok_or(InventoryError::MapIndexRebuildFailed)?
					.publish()
					.map_err(|_| InventoryError::MapIndexRebuildFailed)?;
			}
			MapCache::Reuse(_) => {
				if let Some(update) = cache_update
					&& update.publish().is_err()
				{
					snapshot.incomplete = true;
				}
			}
			MapCache::Rebuild(_) => {}
		}
		let record_value = match self.change {
			RootChange::Keep => None,
			RootChange::Manual => accepted_root.as_ref().map(Some),
			RootChange::Automatic => Some(None),
		};
		if recovering {
			let Some(manual) = record_value else {
				return Ok(blocked_saved_root(self.generation));
			};
			if matches!(self.change, RootChange::Manual)
				&& snapshot.status != InventoryStatus::Ready
			{
				return Ok(blocked_saved_root(self.generation));
			}
			state.root_record_status = if let Some(store) = &self.service.root_record {
				if store.recover(manual).is_err() {
					return Ok(blocked_saved_root(self.generation));
				}
				RootRecordStatus::Current
			} else {
				// An explicit scope can be used for this session without a config directory.
				RootRecordStatus::WriteFailed
			};
		} else if let (Some(store), Some(manual)) = (&self.service.root_record, record_value) {
			state.root_record_status = match store.save(manual) {
				Ok(()) => RootRecordStatus::Current,
				Err(_) => RootRecordStatus::WriteFailed,
			};
		}
		match self.change {
			RootChange::Keep => {
				if accepted_root.is_some() {
					state.manual_root = accepted_root;
				}
			}
			RootChange::Manual => {
				// Keep an invalid explicit selection visible so a refresh cannot use a different scope.
				state.manual_root = accepted_root.or_else(|| self.root.clone());
			}
			RootChange::Automatic => state.manual_root = None,
		}
		snapshot.generation = self.generation.to_string();
		state.banks = banks;
		state.map_members = snapshot.map_members();
		if state.root_record_status == RootRecordStatus::WriteFailed {
			snapshot.incomplete = true;
			snapshot
				.reason
				.get_or_insert(InventoryReason::ConfigurationUnavailable);
		}
		drop(state);
		Ok(snapshot)
	}
}

pub(super) fn collect_inventory(
	manual: Option<&ManualRoot>,
	map_roots: &[PathBuf],
	options: MapScanOptions<'_>,
	ids: &OpaqueIds,
	generation: u64,
	cancellation: &Cancellation,
	progress: &mut InventoryProgressPublisher<impl FnMut(InventoryProgress)>,
) -> Result<CollectedInventory, InventoryError> {
	check_cancelled(cancellation)?;
	progress.locating_folder();
	check_cancelled(cancellation)?;
	let manual_path = manual.map(|root| root.path.as_path());
	if let Some(root) = manual
		&& let Some(identity) = root.identity
		&& (!ordinary_directories(&root.path)
			|| DirectoryIdentity::read(&root.path) != Some(identity))
	{
		return Ok((
			empty_projection(
				manual_path,
				InventoryStatus::Failed,
				Some(InventoryReason::InvalidFolder),
			),
			None,
			None,
		));
	}
	let selected = match manual_path {
		Some(path) => manual_selection(path),
		None => get_sc2_root_path(BANK_SCAN_LIMITS).map(|detected| detected.selection),
	};
	check_cancelled(cancellation)?;
	let selection = match selected {
		Ok(selection) if is_native_disk_path(selection.path()) => selection,
		Ok(_) => {
			return Ok((
				empty_projection(
					manual_path,
					InventoryStatus::Failed,
					Some(InventoryReason::InvalidFolder),
				),
				None,
				None,
			));
		}
		Err(error) => return Ok((path_failure(manual_path, &error), None, None)),
	};
	let root = match AuthorizedRoot::capture(selection.path()) {
		Ok(root) => root,
		Err(reason) => {
			return Ok((
				empty_projection(manual_path, InventoryStatus::Failed, Some(reason)),
				None,
				None,
			));
		}
	};
	let accepted = manual.map(|_| ManualRoot {
		path: root.path().to_owned(),
		identity: Some(root.identity()),
	});
	progress.enumerating_banks();
	check_cancelled(cancellation)?;
	let banks = match inventory_banks(&selection, BANK_SCAN_LIMITS) {
		Ok(banks) => banks,
		Err(error) => return Ok((path_failure(manual_path, &error), accepted, None)),
	};
	check_cancelled(cancellation)?;

	let roots = map_roots
		.iter()
		.filter(|root| is_native_disk_path(root))
		.cloned()
		.collect::<Vec<_>>();
	let cache_path = match options.cache {
		MapCache::Reuse(path) => path,
		MapCache::Rebuild(path) => Some(path),
	};
	let request = IndexRequest {
		roots: &roots,
		cache_path,
		generation,
		limits: IndexLimits {
			max_entries: MAP_SCAN_ENTRIES,
			max_archives: MAP_SCAN_ARCHIVES,
			max_workers: options.max_workers,
			..IndexLimits::default()
		},
	};
	progress.indexing_maps();
	let notify = |update: starbank_map_index::Progress<'_>| progress.inspected_archive(update);
	let result = match options.cache {
		MapCache::Reuse(_) => prepare_map_cache_refresh(request, cancellation, notify),
		MapCache::Rebuild(_) if roots.is_empty() => {
			return Err(InventoryError::MapIndexRebuildFailed);
		}
		MapCache::Rebuild(_) => prepare_map_cache_rebuild(request, cancellation, notify),
	};
	let (report, cache_update) = match result {
		Ok((report, cache_update)) => (Some(report), cache_update),
		Err(ScanError::Cancelled) => return Err(InventoryError::Cancelled),
		Err(_) => (None, None),
	};
	check_cancelled(cancellation)?;
	let mut projection = project(root, manual.is_some(), &banks, report.as_ref(), ids);
	projection.0.can_rebuild_map_index = cache_path.is_some() && !roots.is_empty();
	Ok((projection, accepted, cache_update))
}

fn manual_selection(path: &Path) -> Result<PathSelection, PathError> {
	if !is_native_disk_path(path) {
		return Err(PathError::InvalidSelection(path.to_owned()));
	}
	let canonical = path.canonicalize().map_err(|source| PathError::Io {
		path: path.to_owned(),
		source,
	})?;
	if !is_native_disk_path(&canonical) {
		return Err(PathError::InvalidSelection(path.to_owned()));
	}
	let selected = is_valid_bank_path(&canonical, BANK_SCAN_LIMITS)?;
	if selected.kind() == starbank_paths::SelectionKind::File {
		return Err(PathError::InvalidSelection(path.to_owned()));
	}
	Ok(selected)
}

fn check_cancelled(cancellation: &Cancellation) -> Result<(), InventoryError> {
	if cancellation.is_cancelled() {
		Err(InventoryError::Cancelled)
	} else {
		Ok(())
	}
}

fn path_failure(manual: Option<&Path>, error: &PathError) -> Projection {
	let (status, reason) = match error {
		PathError::NotFound { .. } => (InventoryStatus::RootNotFound, None),
		PathError::NotImplemented | PathError::UnsupportedPlatform => {
			(InventoryStatus::ManualRequired, None)
		}
		PathError::PermissionDenied { .. } => {
			(InventoryStatus::Failed, Some(InventoryReason::Unreadable))
		}
		PathError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => (
			InventoryStatus::Failed,
			Some(InventoryReason::InvalidFolder),
		),
		PathError::LimitReached { .. } => (
			InventoryStatus::Failed,
			Some(InventoryReason::TooManyEntries),
		),
		PathError::InvalidSelection(_) => (
			InventoryStatus::Failed,
			Some(InventoryReason::InvalidFolder),
		),
		_ => (InventoryStatus::Failed, Some(InventoryReason::Unreadable)),
	};
	empty_projection(manual, status, reason)
}

fn empty_projection(
	manual: Option<&Path>,
	status: InventoryStatus,
	reason: Option<InventoryReason>,
) -> Projection {
	let mut snapshot = InventorySnapshot::empty(status, reason);
	snapshot.manual = manual.is_some();
	snapshot.root_name = manual
		.and_then(Path::file_name)
		.map(projection::display_name);
	(snapshot, BTreeMap::new())
}

#[cfg(test)]
mod cache_tests;
