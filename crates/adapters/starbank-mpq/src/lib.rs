//! Bounded read-only access to known map metadata entries in classic MPQ layouts.

#![forbid(unsafe_code)]

mod archive;
mod entry;
mod error;
mod header;

pub use archive::MetadataArchive;
pub use entry::MetadataEntry;
pub use error::{Limit, MpqError};
