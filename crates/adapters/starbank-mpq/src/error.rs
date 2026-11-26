use std::{error::Error, fmt, io};

use crate::entry::MetadataEntry;

/// Resource bound that prevented archive inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
	/// Total archive file length.
	ArchiveBytes,
	/// Number of records in one archive table.
	TableEntries,
	/// Packed or expanded size of the requested metadata entry.
	EntryBytes,
	/// Number of stored sectors in a requested metadata entry.
	EntrySectors,
}

/// Why a local archive or entry could not be inspected.
#[derive(Debug)]
pub enum MpqError {
	/// The archive could not be read or sought.
	Io(io::Error),
	/// The archive format or table layout is outside the supported subset.
	UnsupportedArchive(&'static str),
	/// The requested entry uses unsupported storage flags or a different codec.
	UnsupportedEntry(MetadataEntry),
	/// The archive header or lookup tables are inconsistent.
	InvalidArchive(&'static str),
	/// The requested entry has invalid ranges or an incomplete payload.
	InvalidEntry(MetadataEntry),
	/// A configured resource bound was exceeded.
	LimitExceeded(Limit),
}

impl fmt::Display for MpqError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Io(error) => write!(formatter, "could not read MPQ archive: {error}"),
			Self::UnsupportedArchive(reason) => {
				write!(formatter, "unsupported MPQ archive: {reason}")
			}
			Self::UnsupportedEntry(entry) => write!(
				formatter,
				"unsupported MPQ metadata entry: {}",
				entry.name()
			),
			Self::InvalidArchive(reason) => write!(formatter, "invalid MPQ archive: {reason}"),
			Self::InvalidEntry(entry) => {
				write!(formatter, "invalid MPQ metadata entry: {}", entry.name())
			}
			Self::LimitExceeded(limit) => {
				write!(formatter, "MPQ inspection exceeded the {limit:?} limit")
			}
		}
	}
}

impl Error for MpqError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io(error) => Some(error),
			_ => None,
		}
	}
}

impl From<io::Error> for MpqError {
	fn from(error: io::Error) -> Self {
		Self::Io(error)
	}
}
