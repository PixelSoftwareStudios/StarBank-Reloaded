use std::{
	ffi::{OsStr, OsString},
	fs,
	path::{Path, PathBuf},
};

use cap_std::{ambient_authority, fs::Dir};

use crate::{
	PathError, ScanLimits,
	context::{
		ACCOUNTS, BANK_EXTENSION, BANKS, GAME_DIRECTORY, has_extension, valid_handle,
		valid_numeric_account,
	},
	inventory::{Budget, open_child, scoped_metadata},
};

const VARIABLES: &str = "Variables.txt";
const EXECUTE_INFO: &str = "ExecuteInfo.txt";

/// Scope associated with an account's Banks folder.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlayerScope {
	/// Numeric local account directory, when recognizable.
	pub account: Option<OsString>,
	/// Original lexical SC2 player handle, when recognizable.
	pub player: Option<String>,
}

/// Native type of a selected path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionKind {
	/// StarCraft II's user-data folder.
	GameRoot,
	/// The Accounts directory.
	Accounts,
	/// A selected Banks directory, which may be empty.
	Banks,
	/// A manually selected collection of other structure.
	Collection,
	/// A single selected SC2Bank file.
	File,
}

/// An opened directory authorized for scoped bank discovery.
#[derive(Debug)]
pub struct BankRoot {
	path: PathBuf,
	scope: PlayerScope,
	pub(crate) directory: Dir,
}

impl BankRoot {
	/// Native absolute directory path.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Account and player information derived from known path anchors.
	#[must_use]
	pub fn scope(&self) -> &PlayerScope {
		&self.scope
	}
}

/// A normalized manual selection or auto-detected game root.
#[derive(Debug)]
pub struct PathSelection {
	path: PathBuf,
	kind: SelectionKind,
	pub(crate) roots: Vec<BankRoot>,
	pub(crate) filename: Option<OsString>,
}

impl PathSelection {
	/// Native absolute path after resolving the explicitly selected path.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Whether this selection covers a game root, directory collection, or file.
	#[must_use]
	pub fn kind(&self) -> SelectionKind {
		self.kind
	}

	/// Returns every discovered bank root for this selection.
	#[must_use]
	pub fn bank_roots(&self) -> &[BankRoot] {
		&self.roots
	}
}

/// Normalizes an explicitly selected folder or live bank file.
///
/// File and arbitrary collection selections are accepted independently of SC2
/// installation discovery. Path selection does not grant signing context.
///
/// # Errors
///
/// Reports inaccessible, unsupported or excessive selections with their path.
pub fn is_valid_bank_path(path: &Path, limits: ScanLimits) -> Result<PathSelection, PathError> {
	if path.as_os_str().is_empty() {
		return Err(PathError::InvalidSelection(path.to_owned()));
	}
	let path = fs::canonicalize(path).map_err(|source| PathError::io(path, source))?;
	let meta = fs::metadata(&path).map_err(|source| PathError::io(&path, source))?;
	if meta.is_dir() {
		// Marker files alone are insufficient evidence in an arbitrary manual
		// collection, where they may have been copied alongside live banks.
		let trust_marker_files = named(&path, GAME_DIRECTORY);
		return Ok(classify_directory(path, limits, trust_marker_files)?.0);
	}
	if meta.is_file()
		&& has_extension(&path, BANK_EXTENSION)
		&& let (Some(parent), Some(name)) = (path.parent(), path.file_name())
	{
		let root = BankRoot {
			path: parent.to_owned(),
			scope: scope_from_path(parent),
			directory: open_ambient(parent)?,
		};
		return Ok(PathSelection {
			filename: Some(name.to_owned()),
			path,
			kind: SelectionKind::File,
			roots: vec![root],
		});
	}
	Err(PathError::InvalidSelection(path))
}

#[cfg(any(test, windows, target_os = "macos"))]
pub(crate) fn probe_game_root(
	path: PathBuf,
	limits: ScanLimits,
) -> Result<Option<PathSelection>, PathError> {
	let (mut selection, markers) = classify_directory(path, limits, true)?;
	if !markers {
		return Ok(None);
	}
	selection.kind = SelectionKind::GameRoot;
	Ok(Some(selection))
}

fn classify_directory(
	path: PathBuf,
	limits: ScanLimits,
	trust_marker_files: bool,
) -> Result<(PathSelection, bool), PathError> {
	let directory = open_ambient(&path)?;
	let mut roots = Vec::new();
	let mut markers = false;
	let kind = if named(&path, BANKS) {
		roots.push(BankRoot {
			scope: scope_from_path(&path),
			path: path.clone(),
			directory,
		});
		SelectionKind::Banks
	} else {
		let mut budget = Budget::new(limits);
		if named(&path, ACCOUNTS) {
			find_scoped_roots(&directory, &path, 0, &mut budget, &mut roots)?;
			SelectionKind::Accounts
		} else {
			markers = find_root_markers(
				&directory,
				&path,
				&mut budget,
				&mut roots,
				trust_marker_files,
			)?;
			if markers {
				SelectionKind::GameRoot
			} else {
				roots.push(BankRoot {
					path: path.clone(),
					scope: PlayerScope::default(),
					directory,
				});
				SelectionKind::Collection
			}
		}
	};
	roots.sort_unstable_by(|left, right| left.path.cmp(&right.path));
	Ok((
		PathSelection {
			path,
			kind,
			roots,
			filename: None,
		},
		markers,
	))
}

fn open_ambient(path: &Path) -> Result<Dir, PathError> {
	Dir::open_ambient_dir(path, ambient_authority()).map_err(|source| PathError::io(path, source))
}

fn find_root_markers(
	directory: &Dir,
	parent: &Path,
	budget: &mut Budget,
	roots: &mut Vec<BankRoot>,
	trust_marker_files: bool,
) -> Result<bool, PathError> {
	let mut markers = false;
	let entries = directory
		.entries()
		.map_err(|source| PathError::io(parent, source))?;
	for entry in entries {
		let name = entry
			.map_err(|source| PathError::io(parent, source))?
			.file_name();
		let path = parent.join(&name);
		budget.visit(&path, 0)?;
		if name.eq_ignore_ascii_case(ACCOUNTS) || name.eq_ignore_ascii_case(BANKS) {
			let metadata = scoped_metadata(directory, &name, &path)?;
			if metadata.is_dir() {
				markers = true;
				let child = open_child(directory, &name, &metadata, &path)?;
				if name.eq_ignore_ascii_case(ACCOUNTS) {
					find_scoped_roots(&child, &path, 0, budget, roots)?;
				} else {
					roots.push(BankRoot {
						path,
						scope: PlayerScope::default(),
						directory: child,
					});
				}
			}
		} else if trust_marker_files
			&& (name.eq_ignore_ascii_case(VARIABLES) || name.eq_ignore_ascii_case(EXECUTE_INFO))
		{
			markers |= scoped_metadata(directory, &name, &path)?.is_file();
		}
	}
	Ok(markers)
}

fn find_scoped_roots(
	directory: &Dir,
	parent: &Path,
	level: usize,
	budget: &mut Budget,
	roots: &mut Vec<BankRoot>,
) -> Result<(), PathError> {
	let entries = directory
		.entries()
		.map_err(|source| PathError::io(parent, source))?;
	for entry in entries {
		let name = entry
			.map_err(|source| PathError::io(parent, source))?
			.file_name();
		let path = parent.join(&name);
		budget.visit(&path, level)?;
		let is_banks = name.eq_ignore_ascii_case(BANKS);
		// Accounts may contain a legacy Banks directly, inside an account,
		// or in its normal player directory. Never scan unrelated subtrees.
		if level == 2 && !is_banks {
			continue;
		}
		let metadata = scoped_metadata(directory, &name, &path)?;
		if !metadata.is_dir() {
			continue;
		}
		let child = open_child(directory, &name, &metadata, &path)?;
		if is_banks {
			roots.push(BankRoot {
				scope: scope_from_path(&path),
				path,
				directory: child,
			});
		} else {
			find_scoped_roots(&child, &path, level + 1, budget, roots)?;
		}
	}
	Ok(())
}

fn named(path: &Path, name: &str) -> bool {
	path.file_name()
		.is_some_and(|part| part.eq_ignore_ascii_case(name))
}

fn scope_from_path(path: &Path) -> PlayerScope {
	let banks = path.ancestors().find(|ancestor| named(ancestor, BANKS));
	let Some(parent) = banks.and_then(Path::parent) else {
		return PlayerScope::default();
	};
	let Some(grandparent) = parent.parent() else {
		return PlayerScope::default();
	};
	if named(grandparent, ACCOUNTS) {
		PlayerScope {
			account: parent
				.file_name()
				.filter(|name| valid_numeric_account(name))
				.map(OsStr::to_owned),
			player: None,
		}
	} else if grandparent
		.parent()
		.is_some_and(|ancestor| named(ancestor, ACCOUNTS))
	{
		PlayerScope {
			account: grandparent
				.file_name()
				.filter(|name| valid_numeric_account(name))
				.map(OsStr::to_owned),
			player: parent.file_name().and_then(valid_handle).map(str::to_owned),
		}
	} else {
		PlayerScope::default()
	}
}
