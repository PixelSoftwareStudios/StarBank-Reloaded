//! Native snapshots and verified persistence of already prepared local bytes.
#![deny(unsafe_code)]

mod contract;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod unsupported;
#[cfg(windows)]
mod windows;

pub use contract::{
	CommitIntent, CommitIssue, CommitOutcome, CommitReceipt, FileSnapshot, Fingerprint,
	RecoveryArtifact, StorageError,
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use native::{commit, read_snapshot};
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use unsupported::commit;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub use unsupported::read_snapshot;
#[cfg(windows)]
pub use windows::read_snapshot;
