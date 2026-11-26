//! Native, read-only StarCraft II map metadata and bank association.
#![forbid(unsafe_code)]

mod association;
mod cache;
mod header;
mod metadata;
mod open;
mod paths;
mod scan;
mod script;

pub use association::{MapAssociations, MapIndex, MapRecord};
pub use cache::CacheState;
pub use metadata::{MapMetadata, MapTitleSource, MetadataError, parse_metadata};
pub use paths::known_cache_roots;
pub use scan::{
	Cancellation, IndexIssue, IndexIssueKind, IndexLimit, IndexLimits, IndexReport, IndexRequest,
	Progress, ScanError, scan_maps,
};
