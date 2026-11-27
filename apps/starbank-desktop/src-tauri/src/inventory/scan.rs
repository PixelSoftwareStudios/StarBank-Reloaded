use std::{
	collections::BTreeMap,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use starbank_map_index::{
	Cancellation, IndexLimits, IndexRequest, PreparedCacheUpdate, ScanError,
	prepare_map_cache_rebuild, scan_maps,
};
use starbank_paths::{
	PathError, PathSelection, ScanLimits, get_sc2_root_path, inventory_banks, is_valid_bank_path,
};

use super::{
	InventoryError, InventoryProgress, InventoryReason, InventorySnapshot, InventoryStatus,
	authority::{AuthorizedRoot, DirectoryIdentity, is_native_disk_path, ordinary_directories},
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

type CollectedInventory = (Projection, Option<ManualRoot>, Option<PreparedCacheUpdate>);

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
		MapCache::Reuse(_) => scan_maps(request, cancellation, notify).map(|report| (report, None)),
		MapCache::Rebuild(_) if roots.is_empty() => {
			return Err(InventoryError::MapIndexRebuildFailed);
		}
		MapCache::Rebuild(_) => prepare_map_cache_rebuild(request, cancellation, notify),
	};
	let (report, rebuilt_cache) = match result {
		Ok((report, rebuilt_cache)) => (Some(report), rebuilt_cache),
		Err(ScanError::Cancelled) => return Err(InventoryError::Cancelled),
		Err(_) => (None, None),
	};
	check_cancelled(cancellation)?;
	let mut projection = project(root, manual.is_some(), &banks, report.as_ref(), ids);
	projection.0.can_rebuild_map_index = cache_path.is_some() && !roots.is_empty();
	Ok((projection, accepted, rebuilt_cache))
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

pub(super) fn check_cancelled(cancellation: &Cancellation) -> Result<(), InventoryError> {
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
