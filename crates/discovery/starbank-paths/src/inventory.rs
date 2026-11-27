use std::{
	collections::BTreeMap,
	ffi::OsStr,
	num::NonZeroUsize,
	path::{Path, PathBuf},
};

use cap_std::fs::{Dir, Metadata};

use crate::{
	LimitKind, PathError, PathSelection, PlayerScope, SigningContextHint,
	context::{BACKUP_EXTENSION, BANK_EXTENSION, has_extension},
	parse_signing_context,
	selection::scope_from_path,
};

const DEFAULT_MAX_DEPTH: usize = 64;
const DEFAULT_MAX_ENTRIES: NonZeroUsize = NonZeroUsize::new(100_000).unwrap();

/// Limits for both scope discovery and bank inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanLimits {
	max_depth: usize,
	max_entries: NonZeroUsize,
}

impl ScanLimits {
	/// Sets scan limits. Depth is capped at the default safety ceiling.
	#[must_use]
	pub const fn new(max_depth: usize, max_entries: NonZeroUsize) -> Self {
		Self {
			max_depth: if max_depth < DEFAULT_MAX_DEPTH {
				max_depth
			} else {
				DEFAULT_MAX_DEPTH
			},
			max_entries,
		}
	}
}

impl Default for ScanLimits {
	fn default() -> Self {
		Self {
			max_depth: DEFAULT_MAX_DEPTH,
			max_entries: DEFAULT_MAX_ENTRIES,
		}
	}
}

/// Location and available account context for a bank.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BankLocation {
	/// Native absolute path for display or subsequent explicit file open.
	pub path: PathBuf,
	/// Account and player observed in this bank's path.
	pub scope: PlayerScope,
	/// Context available from the path structure, when unambiguous.
	pub signing_hint: Option<SigningContextHint>,
}

/// A physical bank and the scoped paths that refer to it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BankEntry {
	/// File locations in native path order. Several locations can be hard links.
	pub locations: Vec<BankLocation>,
}

/// Bank inventory within an authorized selection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BankInventory {
	/// Live bank files, including hidden banks and those without map metadata.
	pub banks: Vec<BankEntry>,
	/// Files named with a trailing .sc2bank.bak suffix, kept separate from live banks.
	pub backups: Vec<PathBuf>,
}

/// Enumerates only selected bank roots. No bank XML is opened or parsed.
///
/// # Errors
///
/// Returns a typed path, permission, or limit error when the scan cannot complete.
pub fn inventory_banks(
	selection: &PathSelection,
	limits: ScanLimits,
) -> Result<BankInventory, PathError> {
	let mut state = InventoryState {
		budget: Budget::new(limits),
		banks: BTreeMap::new(),
		backups: Vec::new(),
	};
	for root in &selection.roots {
		if let Some(name) = selection.filename.as_ref() {
			let path = root.path().join(name);
			state.budget.visit(&path, 0)?;
			let metadata = scoped_metadata(&root.directory, name, &path)?;
			if !metadata.is_file() {
				return Err(PathError::EntryChanged(path));
			}
			state.add_file(&root.directory, name, &metadata, path)?;
		} else {
			state.walk(&root.directory, root.path(), 0)?;
		}
	}
	let mut banks = state
		.banks
		.into_values()
		.map(|mut locations| {
			locations.sort_unstable_by(|left, right| left.path.cmp(&right.path));
			locations.dedup();
			BankEntry { locations }
		})
		.collect::<Vec<_>>();
	banks.sort_unstable_by(|left, right| {
		left.locations
			.first()
			.map(|location| &location.path)
			.cmp(&right.locations.first().map(|location| &location.path))
	});
	state.backups.sort_unstable();
	state.backups.dedup();
	Ok(BankInventory {
		banks,
		backups: state.backups,
	})
}

struct InventoryState {
	budget: Budget,
	banks: BTreeMap<FileIdentity, Vec<BankLocation>>,
	backups: Vec<PathBuf>,
}

impl InventoryState {
	fn walk(&mut self, directory: &Dir, parent: &Path, depth: usize) -> Result<(), PathError> {
		let entries = directory
			.entries()
			.map_err(|source| PathError::io(parent, source))?;
		for entry in entries {
			let name = entry
				.map_err(|source| PathError::io(parent, source))?
				.file_name();
			let path = parent.join(&name);
			self.budget.visit(&path, depth)?;
			let metadata = scoped_metadata(directory, &name, &path)?;
			if metadata.is_dir() {
				self.budget.check_depth(&path, depth + 1)?;
				let child = open_child(directory, &name, &metadata, &path)?;
				self.walk(&child, &path, depth + 1)?;
			} else if metadata.is_file() {
				self.add_file(directory, &name, &metadata, path)?;
			}
		}
		Ok(())
	}

	fn add_file(
		&mut self,
		directory: &Dir,
		name: &OsStr,
		metadata: &Metadata,
		path: PathBuf,
	) -> Result<(), PathError> {
		if has_extension(&path, BANK_EXTENSION) {
			let identity = file_identity(directory, name, metadata, &path)?;
			self.banks.entry(identity).or_default().push(BankLocation {
				signing_hint: parse_signing_context(&path),
				scope: path
					.parent()
					.map_or_else(PlayerScope::default, scope_from_path),
				path,
			});
		} else if has_extension(&path, BACKUP_EXTENSION)
			&& path
				.file_stem()
				.is_some_and(|stem| has_extension(Path::new(stem), BANK_EXTENSION))
		{
			self.backups.push(path);
		}
		Ok(())
	}
}

#[derive(Debug)]
pub(crate) struct Budget {
	limits: ScanLimits,
	remaining: usize,
}

impl Budget {
	pub(crate) const fn new(limits: ScanLimits) -> Self {
		Self {
			limits,
			remaining: limits.max_entries.get(),
		}
	}

	pub(crate) fn visit(&mut self, path: &Path, depth: usize) -> Result<(), PathError> {
		self.check_depth(path, depth)?;
		self.remaining = self
			.remaining
			.checked_sub(1)
			.ok_or_else(|| PathError::LimitReached {
				kind: LimitKind::Entries,
				path: path.to_owned(),
			})?;
		Ok(())
	}

	fn check_depth(&self, path: &Path, depth: usize) -> Result<(), PathError> {
		if depth > self.limits.max_depth {
			Err(PathError::LimitReached {
				kind: LimitKind::Depth,
				path: path.to_owned(),
			})
		} else {
			Ok(())
		}
	}
}

pub(crate) fn scoped_metadata(dir: &Dir, name: &OsStr, path: &Path) -> Result<Metadata, PathError> {
	// The only names accepted here are paths returned by directory enumeration or
	// by Path::file_name. Keeping lookup relative to the open parent confines it.
	if !matches!(
		Path::new(name).components().next(),
		Some(std::path::Component::Normal(part)) if part == name
	) {
		return Err(PathError::EntryChanged(path.to_owned()));
	}
	dir.symlink_metadata(name)
		.map_err(|source| PathError::io(path, source))
}

pub(crate) fn open_child(
	dir: &Dir,
	name: &OsStr,
	metadata: &Metadata,
	path: &Path,
) -> Result<Dir, PathError> {
	let child = dir
		.open_dir(name)
		.map_err(|source| PathError::io(path, source))?;
	#[cfg(unix)]
	{
		use cap_std::fs::MetadataExt;
		let opened = child
			.dir_metadata()
			.map_err(|source| PathError::io(path, source))?;
		if (opened.dev(), opened.ino()) != (metadata.dev(), metadata.ino()) {
			return Err(PathError::EntryChanged(path.to_owned()));
		}
	}
	#[cfg(not(unix))]
	let _ = metadata;
	Ok(child)
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg(unix)]
struct FileIdentity(u64, u64);

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg(windows)]
struct FileIdentity(u64, [u8; 16]);

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg(not(any(unix, windows)))]
struct FileIdentity(PathBuf);

#[cfg(unix)]
fn file_identity(
	_dir: &Dir,
	_name: &OsStr,
	metadata: &Metadata,
	_path: &Path,
) -> Result<FileIdentity, PathError> {
	use cap_std::fs::MetadataExt;
	Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(
	dir: &Dir,
	name: &OsStr,
	_metadata: &Metadata,
	path: &Path,
) -> Result<FileIdentity, PathError> {
	let (volume, id) =
		crate::windows::file_identity(dir, name).map_err(|source| PathError::io(path, source))?;
	Ok(FileIdentity(volume, id))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(
	_dir: &Dir,
	_name: &OsStr,
	_metadata: &Metadata,
	path: &Path,
) -> Result<FileIdentity, PathError> {
	Ok(FileIdentity(path.to_owned()))
}
