#![forbid(unsafe_code)]

use std::{
	fs,
	path::{Path, PathBuf},
	time::{Duration, UNIX_EPOCH},
};

use starbank_map_index::{
	CacheState, Cancellation, IndexLimit, IndexLimits, IndexReport, IndexRequest, ScanError,
	parse_metadata, scan_maps,
};
use starbank_testkit::{measure, print_header};

const MIB: usize = 1024 * 1024;
const MAP_WORKLOADS: [(&str, usize, usize, usize); 2] =
	[("small", 4, 1024, 4), ("large", 256, 8 * MIB, 1_000)];
const ARCHIVES: [&[u8]; 3] = [
	include_bytes!("../tests/fixtures/alpha.s2ma"),
	include_bytes!("../tests/fixtures/rival.s2ma"),
	include_bytes!("../tests/fixtures/empty.s2ma"),
];

fn main() {
	print_header();
	for (size, count, script_bytes, references) in MAP_WORKLOADS {
		benchmark_script(size, script_bytes, references);
		benchmark_scan(size, count);
	}
}

fn benchmark_script(size: &str, bytes: usize, references: usize) {
	const REFERENCE: &[u8] = b"BankLoad(bank, 1);\n/*";
	const COMMENT_END: &[u8] = b"*/\n";
	let mut script = Vec::from(&b"// Name: Synthetic Map\nstring bank = \"Reference\";\n"[..]);
	let padding = bytes - script.len() - references * (REFERENCE.len() + COMMENT_END.len());
	for index in 0..references {
		script.extend_from_slice(REFERENCE);
		let extra = usize::from(index < padding % references);
		script.resize(script.len() + padding / references + extra, b'a');
		script.extend_from_slice(COMMENT_END);
	}
	assert_eq!(script.len(), bytes);
	measure(
		"parse_map_script",
		size,
		script.len(),
		|| (),
		|_| parse_metadata(Some(&script), None).expect("valid synthetic map metadata"),
		|_, metadata| {
			assert_eq!(metadata.bank_names, ["reference"]);
			assert_eq!(metadata.title.as_deref(), Some("Synthetic Map"));
		},
	);
}

fn benchmark_scan(size: &str, count: usize) {
	let sandbox = tempfile::tempdir().expect("isolated synthetic map directory");
	let root = sandbox.path().join("maps");
	let cache = sandbox.path().join("metadata.json");
	let input_bytes = write_archives(&root, count);
	let roots = [root];
	let scan = |request| scan_maps(request, &Cancellation::default(), |_| {});
	let uncached = IndexRequest {
		roots: &roots,
		cache_path: None,
		generation: 1,
		limits: IndexLimits::default(),
	};
	let warm = IndexRequest {
		cache_path: Some(&cache),
		..uncached
	};

	measure(
		"index_without_metadata_cache",
		size,
		input_bytes,
		|| (),
		|_| scan(uncached).expect("synthetic archive scan"),
		|_, report| assert_report(&report, count, CacheState::Disabled, 0),
	);
	let initial = scan(warm).expect("populate metadata cache");
	assert_report(&initial, count, CacheState::Missing, 0);
	measure(
		"index_with_warm_metadata_cache",
		size,
		input_bytes,
		|| (),
		|_| scan(warm).expect("scan with populated metadata cache"),
		|_, report| assert_report(&report, count, CacheState::Loaded, count),
	);

	let restricted = IndexRequest {
		limits: IndexLimits {
			max_archives: count - 1,
			..warm.limits
		},
		..warm
	};
	let original_cache = fs::read(&cache).expect("populated metadata cache");
	measure(
		"index_archive_count_limit",
		size,
		input_bytes,
		|| (),
		|_| scan(restricted),
		|_, result| {
			assert!(matches!(
				result,
				Err(ScanError::Limit(IndexLimit::Archives))
			));
			assert_eq!(
				fs::read(&cache).expect("preserved metadata cache"),
				original_cache
			);
		},
	);
}

fn assert_report(report: &IndexReport, count: usize, state: CacheState, hits: usize) {
	assert_eq!(report.index.maps().len(), count);
	assert_eq!(report.cache_state, state);
	assert_eq!(report.cache_hits, hits);
	assert!(report.issues.is_empty() && !report.incomplete);
}

fn write_archives(root: &Path, count: usize) -> usize {
	fs::create_dir(root).expect("create local synthetic archive directory");
	let mut input_bytes = 0;
	for index in 0..count {
		let bytes = ARCHIVES[index % ARCHIVES.len()];
		let path: PathBuf = root.join(format!("synthetic-{index:03}.s2ma"));
		fs::write(&path, bytes).expect("write local synthetic archive");
		// Match the stable archive mtimes used by the map index fixture tests.
		fs::OpenOptions::new()
			.write(true)
			.open(path)
			.expect("open local synthetic archive")
			.set_modified(UNIX_EPOCH + Duration::from_secs(60))
			.expect("set local synthetic archive mtime");
		input_bytes += bytes.len();
	}
	input_bytes
}
