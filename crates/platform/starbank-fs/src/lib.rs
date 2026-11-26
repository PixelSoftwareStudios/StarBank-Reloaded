//! Native snapshots and verified persistence of already prepared local bytes.
#![deny(unsafe_code)]

#[cfg(not(target_family = "wasm"))]
mod backup_catalog;
#[cfg(not(target_family = "wasm"))]
mod backup_directory;
#[cfg(not(target_family = "wasm"))]
mod backup_metadata;
#[cfg(not(target_family = "wasm"))]
mod backup_storage;
mod contract;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(not(target_family = "wasm"))]
pub use backup_catalog::{
	BackupVerification, NativeBackupCatalog, NativeBackupEffect, NativeBackupError,
	NativeBackupPolicy, PreparedRestore, backup_source,
};
#[cfg(not(target_family = "wasm"))]
pub use backup_directory::BackupDirectory;
#[cfg(not(target_family = "wasm"))]
pub use backup_metadata::BackupReasonCode;
pub use contract::{
	CommitIntent, CommitIssue, CommitOutcome, CommitReceipt, FileSnapshot, Fingerprint,
	RecoveryArtifact, StorageError,
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use native::{check_replace_scope, commit, read_snapshot};
#[cfg(not(target_family = "wasm"))]
pub use starbank_backup::{
	BackupHealth, BackupId, BackupPlacement, BackupRecord, BackupStatus, BankSource,
	RetentionPolicy,
};
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub use unsupported::read_snapshot;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use unsupported::{check_replace_scope, commit};
#[cfg(windows)]
pub use windows::read_snapshot;
