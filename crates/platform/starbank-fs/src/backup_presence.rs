use std::{
	collections::{BTreeMap, HashMap, HashSet},
	io,
	path::{Path, PathBuf},
};

use serde::Serialize;
use starbank_backup::{BackupPlacement, BankSource};

use crate::{
	BackupDirectory, StorageError,
	backup_metadata::{METADATA_SUFFIX, Metadata, MetadataScope, source_scope},
	backup_storage::NativeStorage,
};

const MAX_SOURCES: usize = 4_096;
const MAX_MANAGED_FOLDERS: usize = 16;
const MAX_ENTRIES_PER_DIRECTORY: usize = 10_000;
const MAX_ENTRIES: usize = 65_536;
const MAX_SIDECARS: usize = 8_192;

/// Observed history from metadata alone. Payload integrity is a separate check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupPresence {
	/// At least one sidecar recorded a snapshot for this source.
	Recorded,
	/// No snapshots were recorded in any of the supplied authorized locations.
	NeverBackedUp,
	/// Available observations cannot establish the absence of recorded snapshots.
	Unknown,
}

impl BackupPresence {
	fn mark_unknown(&mut self) {
		if *self == Self::NeverBackedUp {
			*self = Self::Unknown;
		}
	}
}

/// Managed locations whose history belongs in this inventory.
#[derive(Clone, Copy, Debug)]
pub enum BackupPresenceDirectory<'a> {
	/// The application's private directory. A directory not yet created is empty.
	Default(&'a Path),
	/// An explicitly authorized directory, which must still match its grant.
	Selected(&'a BackupDirectory),
}

impl<'a> BackupPresenceDirectory<'a> {
	fn path(self) -> &'a Path {
		match self {
			Self::Default(path) => path,
			Self::Selected(grant) => grant.path(),
		}
	}
}

#[derive(Default)]
struct Folder<'a> {
	managed: Option<BackupPresenceDirectory<'a>>,
	adjacent: Vec<usize>,
}

#[derive(Default)]
struct Stems<'a>(HashMap<&'a str, Vec<usize>>);

impl<'a> Stems<'a> {
	fn from_positions(
		sources: &[&'a BankSource],
		positions: impl IntoIterator<Item = usize>,
	) -> Self {
		let mut stems = Self::default();
		for position in positions {
			if let Some(stem) = sources[position]
				.path()
				.file_stem()
				.and_then(|stem| stem.to_str())
			{
				stems.0.entry(stem).or_default().push(position);
			}
		}
		stems
	}
}

struct Scan<'a> {
	sources: Vec<&'a BankSource>,
	by_path: HashMap<&'a Path, usize>,
	by_digest: HashMap<String, usize>,
	states: Vec<BackupPresence>,
	entries_left: usize,
	sidecars_left: usize,
}

/// Observes recorded history for each source in input order.
///
/// Visits each distinct adjacent or managed directory once. Only relevant bounded
/// sidecars are read. Missing, unreadable, unsupported or truncated observations
/// produce `Unknown` unless a valid record already establishes presence.
#[must_use]
pub fn scan_backup_presence(
	sources: &[BankSource],
	managed: &[BackupPresenceDirectory<'_>],
) -> Vec<BackupPresence> {
	if sources.len() > MAX_SOURCES || managed.len() > MAX_MANAGED_FOLDERS {
		return vec![BackupPresence::Unknown; sources.len()];
	}
	if sources.is_empty() {
		return Vec::new();
	}
	let mut scan = Scan {
		sources: Vec::new(),
		by_path: HashMap::new(),
		by_digest: HashMap::new(),
		states: Vec::new(),
		entries_left: MAX_ENTRIES,
		sidecars_left: MAX_SIDECARS,
	};
	let mut folders = BTreeMap::<&Path, Folder<'_>>::new();
	let mut positions = Vec::with_capacity(sources.len());
	for source in sources {
		let next = scan.sources.len();
		let position = *scan.by_path.entry(source.path()).or_insert(next);
		positions.push(position);
		if position == next {
			scan.sources.push(source);
			scan.by_digest.insert(source_scope(source), position);
			scan.states.push(BackupPresence::NeverBackedUp);
			if let Some(parent) = source.path().parent() {
				folders.entry(parent).or_default().adjacent.push(position);
			} else {
				scan.states[position] = BackupPresence::Unknown;
			}
		}
	}
	for &directory in managed {
		let folder = folders.entry(directory.path()).or_default();
		if !matches!(folder.managed, Some(BackupPresenceDirectory::Selected(_))) {
			folder.managed = Some(directory);
		}
	}
	let all_stems = Stems::from_positions(&scan.sources, 0..scan.sources.len());
	for (path, folder) in folders {
		let complete = scan.visit_folder(path, &folder, &all_stems);
		if !complete {
			if folder.managed.is_some() {
				scan.states
					.iter_mut()
					.for_each(BackupPresence::mark_unknown);
			} else {
				for position in folder.adjacent {
					scan.states[position].mark_unknown();
				}
			}
		}
	}
	positions
		.into_iter()
		.map(|index| scan.states[index])
		.collect()
}

impl Scan<'_> {
	fn visit_folder(&mut self, path: &Path, folder: &Folder<'_>, all_stems: &Stems<'_>) -> bool {
		if self.entries_left == 0 {
			return false;
		}
		#[cfg(windows)]
		if !matches!(folder.managed, Some(BackupPresenceDirectory::Selected(_))) {
			return match crate::windows::backup_entries(path) {
				Ok(entries) => self.inspect_entries(
					path,
					folder,
					all_stems,
					&NativeStorage::new(None),
					entries,
				),
				Err(error) => missing_default(folder, &error),
			};
		}
		let captured;
		let grant = match folder.managed {
			Some(BackupPresenceDirectory::Selected(grant)) => grant,
			_ => match BackupDirectory::capture(path) {
				Ok(grant) => {
					captured = grant;
					&captured
				}
				Err(error) => return missing_default(folder, &error),
			},
		};
		let Ok(entries) = grant.entries() else {
			return false;
		};
		self.inspect_entries(
			path,
			folder,
			all_stems,
			&NativeStorage::new(Some(grant)),
			entries,
		) && grant.validate().is_ok()
	}

	fn inspect_entries(
		&mut self,
		path: &Path,
		folder: &Folder<'_>,
		all_stems: &Stems<'_>,
		storage: &NativeStorage<'_>,
		entries: impl Iterator<Item = Result<PathBuf, StorageError>>,
	) -> bool {
		let local_stems;
		let stems = if folder.managed.is_some() {
			all_stems
		} else {
			local_stems = Stems::from_positions(&self.sources, folder.adjacent.iter().copied());
			&local_stems
		};
		let mut suspect_stems = HashSet::new();
		let mut complete = true;
		for (position, entry) in entries.enumerate() {
			if position >= MAX_ENTRIES_PER_DIRECTORY || self.entries_left == 0 {
				complete = false;
				break;
			}
			self.entries_left -= 1;
			let Ok(entry) = entry else {
				complete = false;
				break;
			};
			let Some(prefix) = entry
				.file_name()
				.and_then(|name| name.to_str())
				.and_then(|name| name.strip_suffix(METADATA_SUFFIX))
			else {
				continue;
			};
			let mut matching_stems = prefix
				.match_indices('_')
				.filter_map(|(index, _)| {
					stems
						.0
						.get_key_value(&prefix[..index])
						.map(|(&stem, _)| stem)
				})
				.peekable();
			if matching_stems.peek().is_none() {
				continue;
			}
			if self.sidecars_left == 0 {
				suspect_stems.extend(matching_stems);
				continue;
			}
			self.sidecars_left -= 1;
			if let Ok(metadata) = Metadata::read_at(&entry, storage) {
				let target = match metadata.scope() {
					Some(MetadataScope::Adjacent(name)) => {
						let candidate = path.join(name);
						(candidate.parent() == Some(path))
							.then(|| self.by_path.get(candidate.as_path()).copied())
							.flatten()
					}
					Some(MetadataScope::Managed(digest)) if folder.managed.is_some() => {
						if let Some(&target) = self.by_digest.get(digest) {
							Some(target)
						} else {
							// A valid scope for a different source says nothing about these banks.
							continue;
						}
					}
					_ => None,
				};
				if let Some(index) = target {
					let source = self.sources[index];
					let placement = BackupPlacement::Managed(path);
					if matches!(metadata.record_for(source, placement, &entry), Ok(Some(_))) {
						self.states[index] = BackupPresence::Recorded;
					} else if matches!(metadata.scope(), Some(MetadataScope::Managed(_))) {
						// A readable managed scope isolates the defective record.
						if source
							.path()
							.file_stem()
							.and_then(|stem| stem.to_str())
							.is_some_and(|stem| matching_stems.any(|matching| matching == stem))
						{
							self.states[index].mark_unknown();
						}
					} else {
						suspect_stems.extend(matching_stems);
					}
					continue;
				}
			}
			suspect_stems.extend(matching_stems);
		}
		for stem in suspect_stems {
			for &index in &stems.0[stem] {
				self.states[index].mark_unknown();
			}
		}
		complete
	}
}

fn missing_default(folder: &Folder<'_>, error: &StorageError) -> bool {
	error.kind() == io::ErrorKind::NotFound
		&& folder.adjacent.is_empty()
		&& matches!(folder.managed, Some(BackupPresenceDirectory::Default(_)))
}
