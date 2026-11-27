//! Bounded map metadata inspection and verification of classic MPQ append candidates.

#![forbid(unsafe_code)]

mod archive;
mod entry;
mod error;
mod header;
mod v1_append;

pub use archive::MetadataArchive;
pub use entry::MetadataEntry;
pub use error::{Limit, MpqError};

pub use v1_append::{
	V1_APPEND_ARCHIVE_LIMIT, V1Addition, V1AdditionIssue, V1AppendError, V1AppendPlan,
	V1LayoutIssue, V1PreservationIssue,
};
