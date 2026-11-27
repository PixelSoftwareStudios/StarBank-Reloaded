use std::{
	collections::BTreeSet,
	fs, io,
	path::{Path, PathBuf},
	sync::Arc,
};

use cap_std::{ambient_authority, fs::Dir};

use crate::{
	Cancellation, IndexIssue, IndexIssueKind, IndexLimit, IndexLimits, ScanError,
	cache::Fingerprint,
};

const SHARED_MAC: &str = "/Users/Shared/Blizzard";
const WINDOWS_FALLBACK: &str = r"C:\ProgramData";
const CACHE_DIRECTORY: &str = "Cache";

/// Known current-user native map cache candidates without scanning the filesystem.
///
/// Manual roots can be supplied on every target, including Linux.
pub fn known_cache_roots() -> Vec<PathBuf> {
	match std::env::consts::OS {
		"macos" => shared_candidates(Path::new(SHARED_MAC)),
		"windows" => {
			let program_data = std::env::var_os("ProgramData")
				.map(PathBuf::from)
				.filter(|path| path.is_absolute())
				.unwrap_or_else(|| PathBuf::from(WINDOWS_FALLBACK));
			shared_candidates(&program_data.join("Blizzard Entertainment"))
		}
		_ => Vec::new(),
	}
}

fn shared_candidates(shared: &Path) -> Vec<PathBuf> {
	vec![
		shared
			.join("StarCraft II")
			.join("Maps")
			.join(CACHE_DIRECTORY),
		shared.join("Battle.net").join(CACHE_DIRECTORY),
	]
}

pub(crate) struct ArchiveWork {
	pub(crate) path: PathBuf,
	pub(crate) relative: PathBuf,
	pub(crate) scope: Arc<Dir>,
	pub(crate) fingerprint: Fingerprint,
}

pub(crate) struct Enumeration {
	pub(crate) input_roots: BTreeSet<PathBuf>,
	pub(crate) work: Vec<ArchiveWork>,
	pub(crate) issues: Vec<IndexIssue>,
	pub(crate) complete: bool,
}

pub(crate) fn enumerate(
	roots: &[PathBuf],
	limits: IndexLimits,
	cancellation: &Cancellation,
) -> Result<Enumeration, ScanError> {
	let mut result = Enumeration {
		input_roots: BTreeSet::new(),
		work: Vec::new(),
		issues: Vec::new(),
		complete: true,
	};
	let mut remaining = limits.max_entries;
	let mut seen_archives = BTreeSet::new();
	for root in roots {
		cancellation.check()?;
		remaining = remaining
			.checked_sub(1)
			.ok_or(ScanError::Limit(IndexLimit::Entries))?;
		let is_file = match fs::metadata(root) {
			Ok(metadata) if metadata.is_dir() || metadata.is_file() => metadata.is_file(),
			Ok(_) => {
				result.path_error(
					root,
					io::Error::new(
						io::ErrorKind::InvalidInput,
						"map cache root is not a directory or regular archive",
					),
				);
				continue;
			}
			Err(source) if source.kind() == io::ErrorKind::NotFound => continue,
			Err(source) => {
				result.path_error(root, source);
				continue;
			}
		};
		let canonical = match fs::canonicalize(root) {
			Ok(path) => path,
			Err(source) => {
				result.path_error(root, source);
				continue;
			}
		};
		if !result.input_roots.insert(canonical.clone()) {
			continue;
		}
		let Some(cache_root) = (if is_file {
			canonical.parent()
		} else {
			Some(canonical.as_path())
		}) else {
			continue;
		};
		let scope = match Dir::open_ambient_dir(cache_root, ambient_authority()) {
			Ok(scope) => Arc::new(scope),
			Err(source) => {
				result.path_error(root, source);
				continue;
			}
		};
		let mut scan = Enumerator {
			cancellation,
			limits,
			remaining: &mut remaining,
			seen_archives: &mut seen_archives,
			result: &mut result,
			scope,
			root: cache_root,
		};
		if is_file {
			if let Some(name) = canonical.file_name() {
				scan.archive(Path::new(name))?;
			}
		} else {
			scan.directory(Path::new(""), 0)?;
		}
	}
	result
		.work
		.sort_unstable_by(|left, right| left.path.cmp(&right.path));
	Ok(result)
}

struct Enumerator<'a> {
	cancellation: &'a Cancellation,
	limits: IndexLimits,
	remaining: &'a mut usize,
	seen_archives: &'a mut BTreeSet<PathBuf>,
	result: &'a mut Enumeration,
	scope: Arc<Dir>,
	root: &'a Path,
}

impl Enumerator<'_> {
	fn directory(&mut self, relative: &Path, depth: usize) -> Result<(), ScanError> {
		if depth > self.limits.max_depth {
			return Err(ScanError::Limit(IndexLimit::Depth));
		}
		let entries = if relative.as_os_str().is_empty() {
			self.scope.entries()
		} else {
			self.scope.read_dir(relative)
		};
		let entries = match entries {
			Ok(entries) => entries,
			Err(source) => {
				self.result.path_error(&self.root.join(relative), source);
				return Ok(());
			}
		};
		for entry in entries {
			self.cancellation.check()?;
			*self.remaining = self
				.remaining
				.checked_sub(1)
				.ok_or(ScanError::Limit(IndexLimit::Entries))?;
			let child = match entry {
				Ok(entry) => relative.join(entry.file_name()),
				Err(source) => {
					self.result.path_error(&self.root.join(relative), source);
					continue;
				}
			};
			if is_archive(&child) {
				match Fingerprint::at(&self.scope, &child) {
					Ok(Some(fingerprint)) => self.add_archive(&child, fingerprint)?,
					Ok(None) => {}
					Err(source) => self.result.path_error(&self.root.join(&child), source),
				}
				continue;
			}
			let metadata = match self.scope.symlink_metadata(&child) {
				Ok(metadata) => metadata,
				Err(source) => {
					self.result.path_error(&self.root.join(&child), source);
					continue;
				}
			};
			if metadata.is_dir() {
				self.directory(&child, depth + 1)?;
			}
		}
		Ok(())
	}

	fn archive(&mut self, relative: &Path) -> Result<(), ScanError> {
		if !is_archive(relative) {
			self.result.path_error(
				&self.root.join(relative),
				io::Error::new(
					io::ErrorKind::InvalidInput,
					"unsupported map archive extension",
				),
			);
			return Ok(());
		}
		match Fingerprint::at(&self.scope, relative) {
			Ok(Some(fingerprint)) => self.add_archive(relative, fingerprint),
			Ok(None) => {
				self.result.path_error(
					&self.root.join(relative),
					io::Error::new(
						io::ErrorKind::InvalidInput,
						"map archive is not a regular file",
					),
				);
				Ok(())
			}
			Err(source) => {
				self.result.path_error(&self.root.join(relative), source);
				Ok(())
			}
		}
	}

	fn add_archive(&mut self, relative: &Path, fingerprint: Fingerprint) -> Result<(), ScanError> {
		let path = self.root.join(relative);
		if !self.seen_archives.insert(path.clone()) {
			return Ok(());
		}
		if self.seen_archives.len() > self.limits.max_archives {
			return Err(ScanError::Limit(IndexLimit::Archives));
		}
		if fingerprint.len > self.limits.max_archive_bytes {
			self.result.issues.push(IndexIssue {
				path,
				kind: IndexIssueKind::ArchiveTooLarge,
			});
			return Ok(());
		}
		self.result.work.push(ArchiveWork {
			path,
			relative: relative.to_owned(),
			scope: Arc::clone(&self.scope),
			fingerprint,
		});
		Ok(())
	}
}

impl Enumeration {
	fn path_error(&mut self, path: &Path, source: io::Error) {
		self.complete = false;
		self.issues.push(IndexIssue {
			path: path.to_owned(),
			kind: IndexIssueKind::Io(source),
		});
	}
}

pub(crate) fn is_archive(path: &Path) -> bool {
	path.extension().is_some_and(|extension| {
		["s2ma", "sc2map", "sc2mod"]
			.iter()
			.any(|supported| extension.eq_ignore_ascii_case(supported))
	})
}
