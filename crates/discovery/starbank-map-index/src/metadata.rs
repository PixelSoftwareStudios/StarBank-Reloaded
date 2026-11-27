use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{header, script};

const MAX_SCRIPT_BYTES: usize = 16 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_DISPLAY_BYTES: usize = 1024;
pub(crate) const MAX_BANK_NAME_BYTES: usize = 256;
pub(crate) const MAX_BANK_NAMES: usize = 512;
const BLIZZARD_AUTHOR: &str = "Blizzard Entertainment";
const UTF8_BOM: &[u8] = &[0xef, 0xbb, 0xbf];

/// Source of a reported map title.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MapTitleSource {
	/// Read from Galaxy script header comments as text.
	Script,
	/// Read from the supported DocumentHeader title marker.
	DocumentHeader,
}

/// Metadata decoded without running the Galaxy script.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapMetadata {
	/// Display title, when present and well formed.
	pub title: Option<String>,
	/// Source of the title.
	pub title_source: Option<MapTitleSource>,
	/// Display author declared by the script.
	pub author: Option<String>,
	/// Normalized declared bank names, deduplicated and sorted.
	pub bank_names: Vec<String>,
	/// Whether a script entry could be decoded as text.
	pub has_script: bool,
}

impl MapMetadata {
	/// Reports the author's declared name when available.
	///
	/// This reflects archive metadata and does not verify authorship.
	pub fn declared_blizzard(&self) -> Option<bool> {
		self.author
			.as_deref()
			.map(|author| author.eq_ignore_ascii_case(BLIZZARD_AUTHOR))
	}

	pub(crate) fn is_well_formed(&self) -> bool {
		self.title.is_some() == self.title_source.is_some()
			&& self
				.title
				.as_deref()
				.is_none_or(|value| display_text(value) == Some(value))
			&& self
				.author
				.as_deref()
				.is_none_or(|value| display_text(value) == Some(value))
			&& self.bank_names.len() <= MAX_BANK_NAMES
			&& self.bank_names.is_sorted()
			&& self.bank_names.windows(2).all(|pair| pair[0] != pair[1])
			&& self
				.bank_names
				.iter()
				.all(|name| normalized_bank_name(name).as_ref() == Some(name))
	}
}

/// Why an archive metadata entry could not be decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataError {
	/// Galaxy script exceeds its byte limit.
	ScriptTooLarge,
	/// Galaxy script is not complete UTF-8.
	ScriptEncoding,
	/// DocumentHeader exceeds its byte limit.
	HeaderTooLarge,
	/// Static bank names or variable references exceed their bounds.
	TooManyBankReferences,
}

impl fmt::Display for MetadataError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(match self {
			Self::ScriptTooLarge => "Galaxy script exceeds the metadata limit",
			Self::ScriptEncoding => "Galaxy script is not complete UTF-8",
			Self::HeaderTooLarge => "DocumentHeader exceeds the metadata limit",
			Self::TooManyBankReferences => "Galaxy script exceeds the bank-reference limit",
		})
	}
}

impl Error for MetadataError {}

/// Extracts metadata from full, bounded archive entries.
///
/// Unknown fields remain absent. A script title containing replacement characters
/// is replaced only when DocumentHeader supplies a usable title.
///
/// # Errors
///
/// Reports oversized entries, invalid script encoding, and excessive bank references.
pub fn parse_metadata(
	script_bytes: Option<&[u8]>,
	header_bytes: Option<&[u8]>,
) -> Result<MapMetadata, MetadataError> {
	if script_bytes.is_some_and(|bytes| bytes.len() > MAX_SCRIPT_BYTES) {
		return Err(MetadataError::ScriptTooLarge);
	}
	if header_bytes.is_some_and(|bytes| bytes.len() > MAX_HEADER_BYTES) {
		return Err(MetadataError::HeaderTooLarge);
	}
	let mut metadata = match script_bytes {
		Some(bytes) => {
			let (metadata, bank_names) = parse_script(bytes)?;
			MapMetadata {
				bank_names: bank_names?,
				..metadata
			}
		}
		None => MapMetadata::default(),
	};
	if metadata
		.title
		.as_deref()
		.is_none_or(|title| title.contains(['?', '\u{fffd}']))
		&& let Some(title) = header_bytes.and_then(header::title)
	{
		metadata.title = Some(title.to_owned());
		metadata.title_source = Some(MapTitleSource::DocumentHeader);
	}
	Ok(metadata)
}

pub(crate) fn parse_script(
	bytes: &[u8],
) -> Result<(MapMetadata, Result<Vec<String>, MetadataError>), MetadataError> {
	if bytes.len() > MAX_SCRIPT_BYTES {
		return Err(MetadataError::ScriptTooLarge);
	}
	let source = str::from_utf8(bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes))
		.map_err(|_| MetadataError::ScriptEncoding)?;
	let (title, author) = script::headings(source);
	let metadata = MapMetadata {
		title: title.map(str::to_owned),
		title_source: title.map(|_| MapTitleSource::Script),
		author: author.map(str::to_owned),
		has_script: true,
		..MapMetadata::default()
	};
	Ok((metadata, script::bank_names(source)))
}

pub(crate) fn display_text(value: &str) -> Option<&str> {
	let trimmed = value.trim();
	(!trimmed.is_empty()
		&& trimmed.len() <= MAX_DISPLAY_BYTES
		&& !trimmed.chars().any(char::is_control))
	.then_some(trimmed)
}

pub(crate) fn normalized_bank_name(value: &str) -> Option<String> {
	if value.is_empty()
		|| value.len() > MAX_BANK_NAME_BYTES
		|| value.contains(['/', '\\'])
		|| value.chars().any(char::is_control)
	{
		return None;
	}
	let normalized = value.to_lowercase();
	(normalized.len() <= MAX_BANK_NAME_BYTES).then_some(normalized)
}
