use std::{
	error::Error,
	fmt, io,
	num::NonZeroUsize,
	path::{Path, PathBuf},
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicUsize, Ordering},
		mpsc,
	},
	thread,
};

use starbank_mpq::MpqError;

use crate::{
	CacheState, MapIndex, MapMetadata, MapRecord, MetadataError, PreparedCacheUpdate,
	archive_metadata::{ArchiveMetadata, read_archive},
	cache::{Cache, CachePolicy, Fingerprint, load_cache},
	paths::{ArchiveWork, enumerate},
};

const MAX_WORKERS: usize = 4;
const PROGRESS_QUEUE: usize = MAX_WORKERS * 2;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVES: usize = 4096;
const MAX_DEPTH: usize = 32;
const MAX_ENTRIES: usize = 200_000;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;

/// Resource caps for a map scan. Lower values are accepted for local policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexLimits {
	/// Total filesystem entries inspected.
	pub max_entries: usize,
	/// Number of nesting steps below each authorized directory.
	pub max_depth: usize,
	/// Number of distinct archive paths considered.
	pub max_archives: usize,
	/// Maximum input file size per archive.
	pub max_archive_bytes: u64,
	/// Maximum aggregate stored metadata, before adding the inverted index.
	pub max_metadata_bytes: usize,
	/// Optional lower ceiling on map archive workers for this scan.
	pub max_workers: Option<NonZeroUsize>,
}

impl Default for IndexLimits {
	fn default() -> Self {
		Self {
			max_entries: MAX_ENTRIES,
			max_depth: MAX_DEPTH,
			max_archives: MAX_ARCHIVES,
			max_archive_bytes: MAX_ARCHIVE_BYTES,
			max_metadata_bytes: MAX_METADATA_BYTES,
			max_workers: None,
		}
	}
}

impl IndexLimits {
	fn capped(self) -> Self {
		Self {
			max_entries: self.max_entries.min(MAX_ENTRIES),
			max_depth: self.max_depth.min(MAX_DEPTH),
			max_archives: self.max_archives.min(MAX_ARCHIVES),
			max_archive_bytes: self.max_archive_bytes.min(MAX_ARCHIVE_BYTES),
			max_metadata_bytes: self.max_metadata_bytes.min(MAX_METADATA_BYTES),
			max_workers: self.max_workers,
		}
	}
}

/// Request-scoped cancellation signal. A cancelled signal cannot be reset.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
	/// Stops the scan at its next boundary.
	pub fn cancel(&self) {
		self.0.store(true, Ordering::Relaxed);
	}

	/// Returns whether cancellation was requested.
	pub fn is_cancelled(&self) -> bool {
		self.0.load(Ordering::Relaxed)
	}

	pub(crate) fn check(&self) -> Result<(), ScanError> {
		if self.is_cancelled() {
			Err(ScanError::Cancelled)
		} else {
			Ok(())
		}
	}
}

/// User-authorized input and private metadata cache path for one scan.
#[derive(Clone, Copy, Debug)]
pub struct IndexRequest<'a> {
	/// Native directory or map archive paths.
	pub roots: &'a [PathBuf],
	/// Optional versioned metadata cache, in a writable app-owned directory.
	pub cache_path: Option<&'a Path>,
	/// Caller-issued generation, echoed by progress events.
	pub generation: u64,
	/// Per-generation resource bounds, clamped to hard ceilings.
	pub limits: IndexLimits,
}

/// Incremental progress for a generation. Callbacks run on the calling thread.
#[derive(Clone, Copy, Debug)]
pub struct Progress<'a> {
	/// Caller-issued identifier used to discard stale results.
	pub generation: u64,
	/// Number of completed archive work items.
	pub completed: usize,
	/// Number of archive work items permitted by the scan.
	pub total: usize,
	/// Result of this work item when a record was available.
	pub record: Option<&'a MapRecord>,
}

/// Filesystem or archive warning reported by a completed scan.
#[derive(Debug)]
pub struct IndexIssue {
	/// Native path involved in this local operation.
	pub path: PathBuf,
	/// Details of the warning.
	pub kind: IndexIssueKind,
}

/// Cause of a warning scoped to a file, directory, or metadata cache.
#[derive(Debug)]
pub enum IndexIssueKind {
	/// An archive or directory could not be inspected.
	Io(io::Error),
	/// An archive was larger than the configured file cap.
	ArchiveTooLarge,
	/// A discovered archive changed before inspection completed.
	ArchiveChanged,
	/// The archive backend could not read a requested member.
	Archive(MpqError),
	/// The archive member was not valid or exceeded a metadata limit.
	Metadata(MetadataError),
	/// Metadata cache could not be read.
	CacheRead(io::Error),
	/// Metadata cache could not be updated.
	CacheWrite(io::Error),
}

/// Resource whose bound was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexLimit {
	/// Maximum visited entries.
	Entries,
	/// Maximum path depth.
	Depth,
	/// Maximum number of discovered archives.
	Archives,
	/// Aggregate retained metadata ceiling.
	Metadata,
}

/// Scan abandoned before producing a complete result.
#[derive(Debug)]
pub enum ScanError {
	/// A request's cancellation was observed.
	Cancelled,
	/// A scan limit was exceeded.
	Limit(IndexLimit),
	/// The worker pool could not be created.
	Worker(io::Error),
}

impl fmt::Display for ScanError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Cancelled => f.write_str("Map index scan cancelled"),
			Self::Limit(limit) => write!(f, "Map index {limit:?} limit exceeded"),
			Self::Worker(source) => write!(f, "Map index worker start failed: {source}"),
		}
	}
}

impl Error for ScanError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Worker(source) => Some(source),
			_ => None,
		}
	}
}

/// Finished index and observed work for a generation.
#[derive(Debug)]
pub struct IndexReport {
	/// The complete or diagnostically partial observed map index.
	pub index: MapIndex,
	/// Count of archive records reused without opening the archive.
	pub cache_hits: usize,
	/// State of the cache at the beginning of this generation.
	pub cache_state: CacheState,
	/// Whether any selected cache location could not be completely visited.
	pub incomplete: bool,
	/// Warnings for local files that could only be inspected in part or could not be inspected or cached.
	pub issues: Vec<IndexIssue>,
}

/// Builds a read-only map index. Call from a blocking/background task.
///
/// Bank scanning stays independent. The caller can associate its current bank
/// inventory with the returned index even when some map archives were unavailable.
///
/// # Errors
///
/// Cancellation, a limit crossing or a worker startup failure aborts the scan
/// and leaves the previous persisted cache alone.
pub fn scan_maps(
	request: IndexRequest<'_>,
	cancellation: &Cancellation,
	on_progress: impl FnMut(Progress<'_>),
) -> Result<IndexReport, ScanError> {
	let (mut report, update) = prepare_map_cache_refresh(request, cancellation, on_progress)?;
	if let Some(update) = update {
		let path = update.path.clone();
		if let Err(source) = update.publish() {
			report.issues.push(IndexIssue {
				path,
				kind: IndexIssueKind::CacheWrite(source),
			});
			sort_issues(&mut report.issues);
		}
	}
	Ok(report)
}

/// Builds an index with existing cache entries and stages a changed cache for publication.
///
/// The caller can serialize publication with its final cancellation check.
/// A compatible and unchanged cache requires no replacement.
///
/// # Errors
///
/// Cancellation, a limit crossing or a worker startup failure aborts the scan
/// and leaves the previous persisted cache alone.
pub fn prepare_map_cache_refresh(
	request: IndexRequest<'_>,
	cancellation: &Cancellation,
	on_progress: impl FnMut(Progress<'_>),
) -> Result<(IndexReport, Option<PreparedCacheUpdate>), ScanError> {
	inspect_maps(request, cancellation, CachePolicy::Reuse, on_progress)
}

/// Inspects the same authorized map roots without reading the old cache.
///
/// Returns a prepared replacement only if the scan completed without warnings
/// and the new cache could be staged. The caller controls publication.
///
/// # Errors
///
/// Cancellation, a limit crossing or a worker startup failure aborts the scan
/// and leaves the previous persisted cache alone.
pub fn prepare_map_cache_rebuild(
	request: IndexRequest<'_>,
	cancellation: &Cancellation,
	on_progress: impl FnMut(Progress<'_>),
) -> Result<(IndexReport, Option<PreparedCacheUpdate>), ScanError> {
	inspect_maps(request, cancellation, CachePolicy::Rebuild, on_progress)
}

fn inspect_maps(
	request: IndexRequest<'_>,
	cancellation: &Cancellation,
	policy: CachePolicy,
	mut on_progress: impl FnMut(Progress<'_>),
) -> Result<(IndexReport, Option<PreparedCacheUpdate>), ScanError> {
	cancellation.check()?;
	let limits = request.limits.capped();
	let enumeration = enumerate(request.roots, limits, cancellation)?;
	let total = enumeration.work.len();
	let mut issues = enumeration.issues;
	let (cache, cache_state, cache_path) = load_cache(
		request.cache_path,
		&enumeration.input_roots,
		policy,
		&mut issues,
	);
	let mut updated_cache = Cache::default();
	let mut maps = Vec::new();
	let mut retained = 0_usize;
	let mut cache_hits = 0;
	let mut completed = 0;
	let worker_limit = worker_limit(thread::available_parallelism().ok(), limits.max_workers);
	process_work(
		&enumeration.work,
		worker_limit,
		cancellation,
		&cache,
		|position, result| {
			let work = &enumeration.work[position];
			completed += 1;
			let record = match result {
				Ok((ArchiveMetadata { metadata, warnings }, was_cached)) => {
					retained = retained.saturating_add(metadata_size(&work.path, &metadata));
					if retained > limits.max_metadata_bytes {
						return Err(ScanError::Limit(IndexLimit::Metadata));
					}
					cache_hits += usize::from(was_cached);
					// Reread partial records so a cache hit cannot hide their warnings.
					if cache_path.is_some() && warnings.is_empty() {
						updated_cache.insert(
							&work.path,
							work.fingerprint.clone(),
							metadata.clone(),
						);
					}
					issues.extend(warnings.into_iter().map(|kind| IndexIssue {
						path: work.path.clone(),
						kind,
					}));
					maps.push(MapRecord {
						archive: work.path.clone(),
						modified_nanos: work.fingerprint.modified_nanos,
						metadata,
					});
					maps.last()
				}
				Err(kind) => {
					issues.push(IndexIssue {
						path: work.path.clone(),
						kind,
					});
					None
				}
			};
			on_progress(Progress {
				generation: request.generation,
				completed,
				total,
				record,
			});
			Ok(())
		},
	)?;

	cancellation.check()?;
	let mut prepared = None;
	if let Some(path) = cache_path
		&& enumeration.complete
		&& (matches!(policy, CachePolicy::Reuse) || issues.is_empty())
		&& (cache_state != CacheState::Loaded || updated_cache != cache)
	{
		match updated_cache.prepare(path.clone()) {
			Ok(update) => prepared = Some(update),
			Err(source) => issues.push(IndexIssue {
				path,
				kind: IndexIssueKind::CacheWrite(source),
			}),
		}
	}
	sort_issues(&mut issues);
	let report = IndexReport {
		index: MapIndex::from_records(maps),
		cache_hits,
		cache_state,
		incomplete: !enumeration.complete,
		issues,
	};
	cancellation.check()?;
	Ok((report, prepared))
}

fn sort_issues(issues: &mut [IndexIssue]) {
	issues.sort_by(|left, right| left.path.cmp(&right.path));
}

fn worker_limit(available: Option<NonZeroUsize>, requested: Option<NonZeroUsize>) -> usize {
	let automatic = available.map_or(1, |cpus| (cpus.get() / 2).clamp(1, MAX_WORKERS));
	requested.map_or(automatic, |max| automatic.min(max.get()))
}

fn process_work(
	work: &[ArchiveWork],
	worker_limit: usize,
	cancellation: &Cancellation,
	cache: &Cache,
	mut consume: impl FnMut(
		usize,
		Result<(ArchiveMetadata, bool), IndexIssueKind>,
	) -> Result<(), ScanError>,
) -> Result<(), ScanError> {
	let next = AtomicUsize::new(0);
	thread::scope(|scope| {
		let (send, receive) = mpsc::sync_channel(PROGRESS_QUEUE);
		for _ in 0..worker_limit.min(work.len()) {
			let send = send.clone();
			let next = &next;
			thread::Builder::new()
				.spawn_scoped(scope, move || {
					while !cancellation.is_cancelled() {
						let position = next.fetch_add(1, Ordering::Relaxed);
						let Some(work) = work.get(position) else {
							break;
						};
						if send.send((position, process_one(work, cache))).is_err() {
							break;
						}
					}
				})
				.map_err(ScanError::Worker)?;
		}
		drop(send);
		for (position, result) in receive {
			cancellation.check()?;
			consume(position, result)?;
		}
		Ok(())
	})
}

fn process_one(
	work: &ArchiveWork,
	cache: &Cache,
) -> Result<(ArchiveMetadata, bool), IndexIssueKind> {
	if let Some(metadata) = cache.get(&work.path, &work.fingerprint) {
		check_current_archive(work)?;
		return Ok((
			ArchiveMetadata {
				metadata: metadata.clone(),
				warnings: Vec::new(),
			},
			true,
		));
	}
	let mut file = crate::open::archive(&work.scope, &work.relative).map_err(IndexIssueKind::Io)?;
	let opened = Fingerprint::opened(&file).map_err(IndexIssueKind::Io)?;
	if opened.as_ref() != Some(&work.fingerprint) {
		return Err(IndexIssueKind::ArchiveChanged);
	}
	let metadata = read_archive(&mut file)?;
	let checked = Fingerprint::opened(&file).map_err(IndexIssueKind::Io)?;
	check_current_archive(work)?;
	if checked.as_ref() != Some(&work.fingerprint) {
		return Err(IndexIssueKind::ArchiveChanged);
	}
	Ok((metadata, false))
}

fn check_current_archive(work: &ArchiveWork) -> Result<(), IndexIssueKind> {
	let current = Fingerprint::at(&work.scope, &work.relative).map_err(IndexIssueKind::Io)?;
	if current.as_ref() != Some(&work.fingerprint) {
		return Err(IndexIssueKind::ArchiveChanged);
	}
	Ok(())
}

fn metadata_size(path: &Path, metadata: &MapMetadata) -> usize {
	std::mem::size_of::<MapRecord>()
		+ path.as_os_str().as_encoded_bytes().len()
		+ metadata.title.as_ref().map_or(0, String::len)
		+ metadata.author.as_ref().map_or(0, String::len)
		+ metadata
			.bank_names
			.iter()
			.map(|name| std::mem::size_of::<String>() + name.len())
			.sum::<usize>()
}

#[cfg(test)]
mod worker_tests;
