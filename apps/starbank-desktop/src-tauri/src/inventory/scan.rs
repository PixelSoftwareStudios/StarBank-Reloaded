use std::{
	collections::BTreeMap,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use starbank_map_index::{Cancellation, IndexLimits, IndexRequest, ScanError, scan_maps};
use starbank_paths::{
	PathError, PathSelection, ScanLimits, get_sc2_root_path, inventory_banks, is_valid_bank_path,
};

use super::{
	InventoryError, InventoryReason, InventorySnapshot, InventoryStatus,
	authority::{AuthorizedRoot, DirectoryIdentity, is_native_disk_path, ordinary_directories},
	projection::{self, OpaqueIds, Projection, project},
	root_record::ManualRoot,
};

const BANK_SCAN_DEPTH: usize = 32;
const BANK_SCAN_ENTRIES: NonZeroUsize = NonZeroUsize::new(24_000).unwrap();
const BANK_SCAN_LIMITS: ScanLimits = ScanLimits::new(BANK_SCAN_DEPTH, BANK_SCAN_ENTRIES);
const MAP_SCAN_ENTRIES: usize = 65_536;
const MAP_SCAN_ARCHIVES: usize = 2_048;

pub(super) fn collect_inventory(
	manual: Option<&ManualRoot>,
	map_roots: &[PathBuf],
	cache_path: Option<&Path>,
	ids: &OpaqueIds,
	generation: u64,
	cancellation: &Cancellation,
) -> Result<(Projection, Option<ManualRoot>), InventoryError> {
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
			));
		}
		Err(error) => return Ok((path_failure(manual_path, &error), None)),
	};
	let root = match AuthorizedRoot::capture(selection.path()) {
		Ok(root) => root,
		Err(reason) => {
			return Ok((
				empty_projection(manual_path, InventoryStatus::Failed, Some(reason)),
				None,
			));
		}
	};
	let accepted = manual.map(|_| ManualRoot {
		path: root.path().to_owned(),
		identity: Some(root.identity()),
	});
	let banks = match inventory_banks(&selection, BANK_SCAN_LIMITS) {
		Ok(banks) => banks,
		Err(error) => return Ok((path_failure(manual_path, &error), accepted)),
	};
	check_cancelled(cancellation)?;

	let roots = map_roots
		.iter()
		.filter(|root| is_native_disk_path(root))
		.cloned()
		.collect::<Vec<_>>();
	let request = IndexRequest {
		roots: &roots,
		cache_path,
		generation,
		limits: IndexLimits {
			max_entries: MAP_SCAN_ENTRIES,
			max_archives: MAP_SCAN_ARCHIVES,
			..IndexLimits::default()
		},
	};
	let result = scan_maps(request, cancellation, |_| {});
	let maps = match result {
		Ok(report) => Some(report),
		Err(ScanError::Cancelled) => return Err(InventoryError::Cancelled),
		Err(_) => None,
	};
	check_cancelled(cancellation)?;
	Ok((
		project(root, manual.is_some(), &banks, maps.as_ref(), ids),
		accepted,
	))
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
