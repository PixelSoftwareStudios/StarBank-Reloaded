use cap_std::fs::File;
use starbank_mpq::{Limit, MetadataArchive, MetadataEntry, MpqError};

use crate::{IndexIssueKind, MapMetadata, metadata::parse_script, parse_metadata};

pub(crate) struct ArchiveMetadata {
	pub(crate) metadata: MapMetadata,
	pub(crate) warnings: Vec<IndexIssueKind>,
}

pub(crate) fn read_archive(file: &mut File) -> Result<ArchiveMetadata, IndexIssueKind> {
	let mut archive = MetadataArchive::open(file).map_err(IndexIssueKind::Archive)?;
	let mut warnings = Vec::new();
	let mut metadata = MapMetadata::default();
	for entry in [MetadataEntry::MapScript, MetadataEntry::BaseMapScript] {
		let Some(script) = read_optional_member(&mut archive, entry, &mut warnings)? else {
			continue;
		};
		let (headings, bank_names) = match parse_script(&script) {
			Ok(fields) => fields,
			Err(error) => {
				warnings.push(IndexIssueKind::Metadata(error));
				continue;
			}
		};
		metadata.has_script = true;
		// Keep the first readable headings even when the references need a fallback.
		if metadata.title.is_none() {
			metadata.title = headings.title;
			metadata.title_source = headings.title_source;
		}
		if metadata.author.is_none() {
			metadata.author = headings.author;
		}
		match bank_names {
			Ok(bank_names) => {
				metadata.bank_names = bank_names;
				break;
			}
			Err(error) => warnings.push(IndexIssueKind::Metadata(error)),
		}
	}
	if metadata
		.title
		.as_deref()
		.is_none_or(|title| title.contains(['?', '\u{fffd}']))
		&& let Some(header) =
			read_optional_member(&mut archive, MetadataEntry::DocumentHeader, &mut warnings)?
	{
		match parse_metadata(None, Some(&header)) {
			Ok(header) if header.title.is_some() => {
				metadata.title = header.title;
				metadata.title_source = header.title_source;
			}
			Ok(_) => {}
			Err(error) => warnings.push(IndexIssueKind::Metadata(error)),
		}
	}
	Ok(ArchiveMetadata { metadata, warnings })
}

fn read_optional_member(
	archive: &mut MetadataArchive<&mut File>,
	entry: MetadataEntry,
	warnings: &mut Vec<IndexIssueKind>,
) -> Result<Option<Vec<u8>>, IndexIssueKind> {
	match archive.read(entry) {
		Ok(bytes) => Ok(bytes),
		Err(
			error @ (MpqError::UnsupportedEntry(_)
			| MpqError::InvalidEntry(_)
			| MpqError::LimitExceeded(Limit::EntryBytes | Limit::EntrySectors)),
		) => {
			warnings.push(IndexIssueKind::Archive(error));
			Ok(None)
		}
		Err(error) => Err(IndexIssueKind::Archive(error)),
	}
}
