use std::{fs, path::Path};

use starbank_map_index::{
	Cancellation, IndexIssueKind, IndexLimits, IndexRequest, MapTitleSource, MetadataError,
	scan_maps,
};
use starbank_mpq::{MetadataEntry, MpqError};

const ALPHA: &[u8] = include_bytes!("fixtures/alpha.s2ma");
const FALLBACK_SCRIPT: &[u8] = include_bytes!("fixtures/fallback-script.s2ma");
const FALLBACK_HEADER: &[u8] = include_bytes!("fixtures/fallback-header.s2ma");

const INVALID_SCRIPT_WITH_ALTERNATE: &[u8] =
	include_bytes!("fixtures/invalid-script-with-alternate.s2ma");
const INVALID_SCRIPT_WITH_HEADER: &[u8] =
	include_bytes!("fixtures/invalid-script-with-header.s2ma");
const SCRIPT_REFERENCE_LIMIT: &[u8] = include_bytes!("fixtures/script-reference-limit.s2ma");
const REFERENCE_LIMIT_WITH_ALTERNATE: &[u8] =
	include_bytes!("fixtures/reference-limit-with-alternate.s2ma");

#[test]
fn unsupported_primary_members_keep_warnings_and_use_readable_fallbacks()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let root = temp.path().canonicalize()?;
	let maps = root.join("maps");
	fs::create_dir(&maps)?;
	let script = maps.join("alternate.s2ma");
	let header = maps.join("header.s2ma");
	let clean = maps.join("clean.s2ma");
	let malformed = maps.join("short.s2ma");
	fs::write(&script, FALLBACK_SCRIPT)?;
	fs::write(&header, FALLBACK_HEADER)?;
	fs::write(&clean, ALPHA)?;
	fs::write(&malformed, &ALPHA[..20])?;

	let cache = root.join("metadata.json");
	let request = IndexRequest {
		roots: &[maps],
		cache_path: Some(&cache),
		generation: 23,
		limits: IndexLimits::default(),
	};
	let mut updates = Vec::new();
	let first = scan_maps(request, &Cancellation::default(), |update| {
		updates.push((
			update.generation,
			update.completed,
			update.total,
			update.record.is_some(),
		));
	})?;
	let second = scan_maps(request, &Cancellation::default(), |_| {})?;
	assert_eq!(first.index.maps(), second.index.maps());
	assert_eq!(second.cache_hits, 1);
	assert_eq!(updates.len(), 4);
	assert_eq!(updates.iter().filter(|update| update.3).count(), 3);
	assert!(updates.iter().all(|update| update.0 == 23 && update.2 == 4));

	for report in [first, second] {
		let records = report.index.maps();
		assert_eq!(records.len(), 3);
		assert_eq!(report.issues.len(), 3);
		for fallback in [&script, &header] {
			assert!(report.issues.iter().any(|issue| issue.path == *fallback
				&& matches!(
					issue.kind,
					IndexIssueKind::Archive(MpqError::UnsupportedEntry(MetadataEntry::MapScript))
				)));
		}
		assert!(report.issues.iter().any(|issue| issue.path == malformed
			&& matches!(
				issue.kind,
				IndexIssueKind::Archive(MpqError::InvalidArchive(_))
			)));
		let alternate = &records
			.iter()
			.find(|map| map.archive == script)
			.ok_or("alternate map missing")?
			.metadata;
		assert_eq!(alternate.title.as_deref(), Some("Alternate Référence"));
		assert_eq!(alternate.title_source, Some(MapTitleSource::Script));
		assert_eq!(alternate.author.as_deref(), Some("Fixture Reader"));
		assert_eq!(alternate.bank_names, ["fromalternate"]);
		assert!(alternate.has_script);

		let header_only = &records
			.iter()
			.find(|map| map.archive == header)
			.ok_or("header-only map missing")?
			.metadata;
		assert_eq!(header_only.title.as_deref(), Some("Header fallback"));
		assert_eq!(
			header_only.title_source,
			Some(MapTitleSource::DocumentHeader)
		);
		assert!(header_only.author.is_none());
		assert_eq!(header_only.bank_names, Vec::<String>::new());
		assert!(!header_only.has_script);
	}
	Ok(())
}

#[test]
fn invalid_script_metadata_preserves_readable_map_fields_and_reports_warnings()
-> Result<(), Box<dyn std::error::Error>> {
	let scratch = tempfile::tempdir()?;
	let root = scratch.path().canonicalize()?;
	let maps = root.join("Cache");
	fs::create_dir(&maps)?;
	let alternate = maps.join("alternate.s2ma");
	let header_only = maps.join("header.s2ma");
	let reference_limit = maps.join("heading.s2ma");
	let combined = maps.join("combined.s2ma");
	fs::write(&combined, REFERENCE_LIMIT_WITH_ALTERNATE)?;
	fs::write(&alternate, INVALID_SCRIPT_WITH_ALTERNATE)?;
	fs::write(&header_only, INVALID_SCRIPT_WITH_HEADER)?;
	fs::write(&reference_limit, SCRIPT_REFERENCE_LIMIT)?;
	fs::write(maps.join("clean.s2ma"), ALPHA)?;
	let cache = root.join("metadata.json");
	let request = IndexRequest {
		roots: &[maps],
		cache_path: Some(&cache),
		generation: 25,
		limits: IndexLimits::default(),
	};

	let cold = scan_maps(request, &Cancellation::default(), |_| {})?;
	let warm = scan_maps(request, &Cancellation::default(), |_| {})?;
	assert_eq!((cold.cache_hits, warm.cache_hits), (0, 1));
	assert_eq!(cold.index.maps(), warm.index.maps());
	assert_eq!(cold.index.maps().len(), 5);

	for report in [&cold, &warm] {
		assert_eq!(report.issues.len(), 4);
		for (path, expected) in [
			(&alternate, MetadataError::ScriptEncoding),
			(&header_only, MetadataError::ScriptEncoding),
			(&reference_limit, MetadataError::TooManyBankReferences),
			(&combined, MetadataError::TooManyBankReferences),
		] {
			assert!(report.issues.iter().any(|issue| issue.path == *path
				&& matches!(issue.kind, IndexIssueKind::Metadata(error) if error == expected)));
		}
	}
	let metadata_by_path = |path: &Path| {
		cold.index
			.maps()
			.iter()
			.find(|map| map.archive == path)
			.map(|map| &map.metadata)
	};
	let metadata = metadata_by_path(&alternate).expect("readable alternate");
	assert_eq!(metadata.title.as_deref(), Some("Alternate Référence"));
	assert_eq!(metadata.author.as_deref(), Some("Fixture Reader"));
	assert_eq!(metadata.bank_names, ["fromalternate"]);
	let metadata = metadata_by_path(&header_only).expect("readable document title");
	assert_eq!(metadata.title.as_deref(), Some("Header fallback"));
	assert_eq!(metadata.title_source, Some(MapTitleSource::DocumentHeader));
	assert!(!metadata.has_script);
	assert!(metadata.author.is_none() && metadata.bank_names.is_empty());
	let metadata = metadata_by_path(&reference_limit).expect("readable script headings");
	assert_eq!(metadata.title.as_deref(), Some("Bounded fixture headings"));
	assert_eq!(metadata.author.as_deref(), Some("Synthetic Studio"));
	assert!(metadata.has_script && metadata.bank_names.is_empty());
	let metadata = metadata_by_path(&combined).expect("primary headings with alternate names");
	assert_eq!(metadata.title.as_deref(), Some("Bounded fixture headings"));
	assert_eq!(metadata.title_source, Some(MapTitleSource::Script));
	assert_eq!(metadata.author.as_deref(), Some("Synthetic Studio"));
	assert_eq!(metadata.bank_names, ["recoveredbank"]);
	assert!(metadata.has_script);
	Ok(())
}
