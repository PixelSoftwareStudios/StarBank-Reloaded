use std::{
	fs, io,
	path::{Path, PathBuf},
};

use serde::Serialize;
use starbank_map_index::{
	Cancellation, IndexIssue, IndexIssueKind, IndexLimits, IndexReport, IndexRequest, MapRecord,
	MapTitleSource, MetadataError, ScanError, scan_maps,
};
use starbank_mpq::MpqError;

use crate::{
	args::MapCommand,
	discovery::DisplayPath,
	failure::{ErrorCode, Failure, Status},
	output::{self, CommandName},
};

const SC2_MAP_EXTENSION: &str = "sc2map";
const MAP_ROOT_EXTENSIONS: &[&str] = &[SC2_MAP_EXTENSION, "sc2mod", "s2ma"];
const SINGLE_MAP_ITEMS: usize = 1;
const CLI_SCAN_GENERATION: u64 = 1;

pub(crate) fn run(command: MapCommand, json: bool) -> Result<Status, Failure> {
	match command {
		MapCommand::Inspect { file } => inspect(&file, json),
		MapCommand::Scan { root } => scan(&root, json),
	}
}

fn inspect(file: &Path, json: bool) -> Result<Status, Failure> {
	if !has_extension(file, &[SC2_MAP_EXTENSION]) || !fs::symlink_metadata(file)?.is_file() {
		return Err(Failure::new(
			Status::Validation,
			ErrorCode::InvalidMapSelection,
			"Select a regular .SC2Map file.",
		));
	}

	let scope = fs::canonicalize(file)?;
	let limits = IndexLimits {
		max_entries: SINGLE_MAP_ITEMS,
		max_archives: SINGLE_MAP_ITEMS,
		..IndexLimits::default()
	};
	let scanned = read_index(std::slice::from_ref(&scope), limits)?;
	let current = fs::symlink_metadata(file).map_err(|_| changed())?;
	if !current.is_file() || fs::canonicalize(file).map_err(|_| changed())? != scope {
		return Err(changed());
	}
	if let Some(issue) = scanned.issues.first() {
		return Err(index_issue(&issue.kind));
	}
	let [record] = scanned.index.maps() else {
		return Err(changed());
	};
	if record.archive != scope {
		return Err(changed());
	}

	let report = MapReport::new(record);
	output::report(json, CommandName::MapInspect, &report, |out| {
		writeln!(out, "Map: {}", report.title.unwrap_or("Unknown"))?;
		writeln!(
			out,
			"Declared author: {}",
			report.author.unwrap_or("Unknown")
		)?;
		for name in report.bank_names {
			writeln!(out, "Bank: {name:?}")?;
		}
		Ok(())
	})?;
	Ok(Status::Success)
}

fn scan(roots: &[PathBuf], json: bool) -> Result<Status, Failure> {
	for root in roots {
		let metadata = fs::symlink_metadata(root)?;
		if !(metadata.is_dir() || metadata.is_file() && has_extension(root, MAP_ROOT_EXTENSIONS)) {
			return Err(Failure::new(
				Status::Validation,
				ErrorCode::InvalidMapSelection,
				"Select a directory or a regular .SC2Map, .SC2Mod, or .s2ma file as a map root.",
			));
		}
	}
	let scanned = read_index(roots, IndexLimits::default())?;
	let report = ScanReport {
		maps: scanned.index.maps().iter().map(MapReport::new).collect(),
		issues: scanned.issues.iter().map(IssueReport::new).collect(),
		incomplete: scanned.incomplete || !scanned.issues.is_empty(),
	};

	output::report(json, CommandName::MapScan, &report, |out| {
		writeln!(
			out,
			"{} map archives read. {} issues.",
			report.maps.len(),
			report.issues.len()
		)?;
		for map in &report.maps {
			writeln!(
				out,
				"{} ({:?})",
				map.title.unwrap_or("Unknown"),
				map.path.display
			)?;
		}
		for issue in &report.issues {
			writeln!(out, "{} ({:?})", issue.error, issue.path.display)?;
		}
		Ok(())
	})?;
	let status = if report.maps.is_empty() {
		report
			.issues
			.first()
			.map_or(Status::Success, |issue| issue.error.status)
	} else {
		Status::Success
	};
	Ok(status)
}

fn read_index(roots: &[PathBuf], limits: IndexLimits) -> Result<IndexReport, Failure> {
	scan_maps(
		IndexRequest {
			roots,
			cache_path: None,
			generation: CLI_SCAN_GENERATION,
			limits,
		},
		&Cancellation::default(),
		|_| {},
	)
	.map_err(|error| match error {
		ScanError::Limit(_) => over_limit(),
		ScanError::Cancelled | ScanError::Worker(_) => Failure::new(
			Status::Io,
			ErrorCode::MapScanFailed,
			"The selected map inspection could not complete.",
		),
	})
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
	path.extension().is_some_and(|extension| {
		extensions
			.iter()
			.any(|accepted| extension.eq_ignore_ascii_case(accepted))
	})
}

fn index_issue(issue: &IndexIssueKind) -> Failure {
	match issue {
		IndexIssueKind::ArchiveTooLarge
		| IndexIssueKind::Archive(MpqError::LimitExceeded(_))
		| IndexIssueKind::Metadata(
			MetadataError::ScriptTooLarge
			| MetadataError::HeaderTooLarge
			| MetadataError::TooManyBankReferences,
		) => over_limit(),
		IndexIssueKind::ArchiveChanged => changed(),
		IndexIssueKind::Io(error) | IndexIssueKind::Archive(MpqError::Io(error)) => {
			io::Error::from(error.kind()).into()
		}
		IndexIssueKind::Archive(
			MpqError::UnsupportedArchive(_) | MpqError::UnsupportedEntry(_),
		) => Failure::new(
			Status::Unsupported,
			ErrorCode::UnsupportedMapArchive,
			"The selected archive or metadata entry uses an unsupported format.",
		),
		IndexIssueKind::Archive(MpqError::InvalidArchive(_) | MpqError::InvalidEntry(_)) => {
			Failure::new(
				Status::Validation,
				ErrorCode::InvalidMapArchive,
				"The selected archive or metadata entry is invalid.",
			)
		}
		IndexIssueKind::Metadata(MetadataError::ScriptEncoding) => Failure::new(
			Status::Validation,
			ErrorCode::InvalidMapMetadata,
			"The selected archive contains invalid metadata text.",
		),
		IndexIssueKind::CacheRead(_) | IndexIssueKind::CacheWrite(_) => Failure::new(
			Status::Io,
			ErrorCode::MapScanFailed,
			"The selected map inspection could not complete.",
		),
	}
}

fn over_limit() -> Failure {
	Failure::new(
		Status::Validation,
		ErrorCode::InputLimit,
		"Map inspection exceeded a resource limit.",
	)
}

fn changed() -> Failure {
	Failure::new(
		Status::Conflict,
		ErrorCode::InputChanged,
		"The selected map path or archive changed during inspection.",
	)
}

#[derive(Debug, Serialize)]
struct ScanReport<'data> {
	maps: Vec<MapReport<'data>>,
	issues: Vec<IssueReport<'data>>,
	incomplete: bool,
}

#[derive(Debug, Serialize)]
struct IssueReport<'data> {
	path: DisplayPath<'data>,
	error: Failure,
}

impl<'data> IssueReport<'data> {
	fn new(issue: &'data IndexIssue) -> Self {
		Self {
			path: DisplayPath::new(&issue.path),
			error: index_issue(&issue.kind),
		}
	}
}

#[derive(Debug, Serialize)]
struct MapReport<'data> {
	path: DisplayPath<'data>,
	modified_unix_nanos: Option<String>,
	title: Option<&'data str>,
	title_source: Option<TitleSource>,
	author: Option<&'data str>,
	bank_names: &'data [String],
	has_script: bool,
	declared_blizzard: Option<bool>,
}

impl<'data> MapReport<'data> {
	fn new(record: &'data MapRecord) -> Self {
		let metadata = &record.metadata;
		Self {
			path: DisplayPath::new(&record.archive),
			modified_unix_nanos: record.modified_nanos.map(|value| value.to_string()),
			title: metadata.title.as_deref(),
			title_source: metadata.title_source.map(TitleSource::from),
			author: metadata.author.as_deref(),
			bank_names: &metadata.bank_names,
			has_script: metadata.has_script,
			declared_blizzard: metadata.declared_blizzard(),
		}
	}
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum TitleSource {
	Script,
	DocumentHeader,
}

impl From<MapTitleSource> for TitleSource {
	fn from(source: MapTitleSource) -> Self {
		match source {
			MapTitleSource::Script => Self::Script,
			MapTitleSource::DocumentHeader => Self::DocumentHeader,
		}
	}
}
