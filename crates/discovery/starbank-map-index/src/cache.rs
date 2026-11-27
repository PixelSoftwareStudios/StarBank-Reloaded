use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	io::{self, Read, Write},
	path::{Path, PathBuf},
	time::UNIX_EPOCH,
};

use cap_std::fs::Metadata;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{IndexIssue, IndexIssueKind, MapMetadata};

const CACHE_FORMAT: u32 = 1;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn locate(path: &Path, input_roots: &BTreeSet<PathBuf>) -> io::Result<PathBuf> {
	let (Some(directory), Some(filename)) = (path.parent(), path.file_name()) else {
		return Err(io::Error::new(
			io::ErrorKind::InvalidInput,
			"metadata cache path is not a file in an app directory",
		));
	};
	let candidate = directory.canonicalize()?.join(filename);
	if crate::paths::is_archive(&candidate)
		|| input_roots.iter().any(|root| candidate.starts_with(root))
	{
		return Err(io::Error::new(
			io::ErrorKind::InvalidInput,
			"metadata cache must be outside map input roots and archive paths",
		));
	}
	Ok(candidate)
}

#[derive(Clone, Copy)]
pub(crate) enum CachePolicy {
	Reuse,
	Rebuild,
}

pub(crate) fn load_cache(
	requested: Option<&Path>,
	input_roots: &BTreeSet<PathBuf>,
	policy: CachePolicy,
	issues: &mut Vec<IndexIssue>,
) -> (Cache, CacheState, Option<PathBuf>) {
	match requested {
		Some(requested) => {
			let (cache, state, path, error) = match locate(requested, input_roots) {
				Ok(normalized) => {
					let (cache, state, error) = match policy {
						CachePolicy::Reuse => Cache::load(&normalized),
						CachePolicy::Rebuild => (Cache::default(), CacheState::Rebuilding, None),
					};
					(cache, state, Some(normalized), error)
				}
				Err(error) => (Cache::default(), CacheState::Unavailable, None, Some(error)),
			};
			if let Some(source) = error {
				issues.push(IndexIssue {
					path: requested.to_owned(),
					kind: IndexIssueKind::CacheRead(source),
				});
			}
			(
				cache,
				state,
				path.filter(|_| state != CacheState::Unavailable),
			)
		}
		None => (Cache::default(), CacheState::Disabled, None),
	}
}

/// The previous persisted metadata cache's state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheState {
	/// No persistent path was requested.
	Disabled,
	/// No cache file existed.
	Missing,
	/// A compatible cache was loaded.
	Loaded,
	/// The cache was incompatible or deliberately skipped for a rebuild.
	Rebuilding,
	/// A filesystem error prevented reading the cache.
	Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Fingerprint {
	pub(crate) len: u64,
	pub(crate) modified_nanos: Option<i128>,
	identity: Option<(u64, u64)>,
}

impl Fingerprint {
	pub(crate) fn of(metadata: &Metadata) -> Self {
		Self {
			len: metadata.len(),
			modified_nanos: metadata.modified().ok().map(|modified| {
				let time = modified.into_std();
				match time.duration_since(UNIX_EPOCH) {
					Ok(duration) => duration.as_nanos().cast_signed(),
					Err(before) => -before.duration().as_nanos().cast_signed(),
				}
			}),
			identity: physical_identity(metadata),
		}
	}
}

#[cfg(unix)]
fn physical_identity(metadata: &Metadata) -> Option<(u64, u64)> {
	use cap_std::fs::MetadataExt;
	Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn physical_identity(_metadata: &Metadata) -> Option<(u64, u64)> {
	// Windows cache checks use size and modification time as their available
	// fingerprint. A replacement preserving both can match the cached record.
	None
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Cache {
	version: u32,
	records: BTreeMap<String, CacheRecord>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CacheRecord {
	fingerprint: Fingerprint,
	metadata: MapMetadata,
}

impl Default for Cache {
	fn default() -> Self {
		Self {
			version: CACHE_FORMAT,
			records: BTreeMap::new(),
		}
	}
}

impl Cache {
	pub(crate) fn load(path: &Path) -> (Self, CacheState, Option<io::Error>) {
		let bytes = match read_file(path) {
			Ok(bytes) => bytes,
			Err(error) if error.kind() == io::ErrorKind::NotFound => {
				return (Self::default(), CacheState::Missing, None);
			}
			Err(error) if error.kind() == io::ErrorKind::InvalidData => {
				return (Self::default(), CacheState::Rebuilding, None);
			}
			Err(error) => return (Self::default(), CacheState::Unavailable, Some(error)),
		};
		if let Ok(cache) = serde_json::from_slice::<Self>(&bytes)
			&& cache.version == CACHE_FORMAT
			&& cache
				.records
				.values()
				.all(|record| record.metadata.is_well_formed())
		{
			(cache, CacheState::Loaded, None)
		} else {
			(Self::default(), CacheState::Rebuilding, None)
		}
	}

	pub(crate) fn get(&self, path: &Path, fingerprint: &Fingerprint) -> Option<&MapMetadata> {
		fingerprint.modified_nanos?;
		let entry = self.records.get(path.to_str()?)?;
		(&entry.fingerprint == fingerprint).then_some(&entry.metadata)
	}

	pub(crate) fn insert(&mut self, path: &Path, fingerprint: Fingerprint, metadata: MapMetadata) {
		if let Some(path) = path.to_str()
			&& fingerprint.modified_nanos.is_some()
		{
			self.records.insert(
				path.to_owned(),
				CacheRecord {
					fingerprint,
					metadata,
				},
			);
		}
	}

	pub(crate) fn prepare(&self, path: PathBuf) -> io::Result<PreparedCacheUpdate> {
		let directory = path
			.parent()
			.filter(|parent| !parent.as_os_str().is_empty())
			.ok_or_else(|| {
				io::Error::new(
					io::ErrorKind::InvalidInput,
					"cache path requires a parent directory",
				)
			})?;
		let mut temporary = NamedTempFile::new_in(directory)?;
		let writer = LimitedWriter {
			inner: temporary.as_file_mut(),
			remaining: MAX_CACHE_BYTES,
		};
		serde_json::to_writer(writer, self)?;
		temporary.as_file().sync_all()?;
		Ok(PreparedCacheUpdate { temporary, path })
	}
}

/// A fully written, synced index cache awaiting atomic publication.
///
/// Dropping a prepared cache leaves the previous cache intact.
#[derive(Debug)]
pub struct PreparedCacheUpdate {
	temporary: NamedTempFile,
	pub(crate) path: PathBuf,
}

impl PreparedCacheUpdate {
	/// Atomically replaces the app-owned metadata cache with the prepared contents.
	///
	/// # Errors
	///
	/// Returns the filesystem error if replacement fails.
	pub fn publish(self) -> io::Result<()> {
		self.temporary
			.persist(self.path)
			.map_err(|error| error.error)?;
		Ok(())
	}
}

fn read_file(path: &Path) -> io::Result<Vec<u8>> {
	let metadata = fs::symlink_metadata(path)?;
	if !metadata.is_file() || metadata.len() > MAX_CACHE_BYTES as u64 {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"cache is not a bounded regular file",
		));
	}
	let mut bytes = Vec::new();
	crate::open::cache(path)?
		.take(MAX_CACHE_BYTES as u64 + 1)
		.read_to_end(&mut bytes)?;
	if bytes.len() > MAX_CACHE_BYTES {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"cache exceeds its size limit",
		));
	}
	Ok(bytes)
}

struct LimitedWriter<W> {
	inner: W,
	remaining: usize,
}

impl<W: Write> Write for LimitedWriter<W> {
	fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
		if bytes.len() > self.remaining {
			return Err(io::Error::new(
				io::ErrorKind::InvalidData,
				"cache exceeds its size limit",
			));
		}
		let written = self.inner.write(bytes)?;
		self.remaining -= written;
		Ok(written)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.inner.flush()
	}
}
