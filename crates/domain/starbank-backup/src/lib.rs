//! Backup naming, verification, and restore policy over an authorized storage adapter.

#![forbid(unsafe_code)]

mod catalog;
mod naming;
mod operations;
mod record;
mod storage;

pub use catalog::{BackupCatalog, BackupHealth, BackupStatus, CatalogError, RetentionPolicy};
pub use naming::{
	BackupId, BackupPlacement, BackupRequest, BackupTimestamp, BankSource, NamingError,
};
pub use operations::{BackupError, Overwrite, OverwriteBackup, StorageOutcome, WriteOutcome};
pub use record::{BackupIntegrity, BackupReason, BackupRecord};
pub use storage::{
	BackupStorage, CommitState, ReadFailure, RecordFailure, Snapshot, StorageIntent,
};
