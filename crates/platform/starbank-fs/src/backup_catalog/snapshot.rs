use std::path::Path;

use super::{NativeBackupCatalog, NativeBackupError, PreparedRestore, managed_scope};
use crate::{
	BackupDirectory, BackupPlacement, BackupRecord, BankSource, FileSnapshot,
	backup_metadata::{Metadata, backup_directory, sidecar_path},
	backup_storage::NativeStorage,
};

impl NativeBackupCatalog {
	/// Reads the exact bytes of a previously reviewed backup in its authorized folder.
	///
	/// # Errors
	///
	/// Rejects changed metadata, a changed folder, unreadable data, or a mismatched digest.
	pub fn read_verified_backup(
		&self,
		record: &BackupRecord,
		placement: BackupPlacement<'_>,
	) -> Result<FileSnapshot, NativeBackupError> {
		let (_, selected, _) =
			self.read_backup(record.source(), record.id().path(), placement, Some(record))?;
		Ok(selected)
	}

	/// Checks the selected snapshot and its sidecar before preparing a live restore.
	///
	/// Supply the current bank snapshot observed before confirmation.
	///
	/// # Errors
	///
	/// Rejects a mismatched scope, invalid sidecar, unreadable snapshot or incorrect digest.
	pub fn prepare_restore(
		&self,
		source: &BankSource,
		current: FileSnapshot,
		backup: &Path,
		placement: BackupPlacement<'_>,
	) -> Result<PreparedRestore, NativeBackupError> {
		let (record, selected, selected_directory) =
			self.read_backup(source, backup, placement, None)?;
		Ok(PreparedRestore {
			record,
			current,
			selected,
			selected_directory,
		})
	}

	fn read_backup(
		&self,
		source: &BankSource,
		backup: &Path,
		placement: BackupPlacement<'_>,
		expected: Option<&BackupRecord>,
	) -> Result<(BackupRecord, FileSnapshot, Option<BackupDirectory>), NativeBackupError> {
		if backup.parent() != Some(backup_directory(source, placement)?) {
			return Err(NativeBackupError::MetadataMismatch);
		}
		let scope = managed_scope(self.managed_directory.as_ref(), placement)?;
		let storage = NativeStorage::new(scope);
		let record = Metadata::read(source, placement, &sidecar_path(backup), &storage)?
			.ok_or(NativeBackupError::MetadataMismatch)?;
		let integrity = record.integrity();
		if record.id().path() != backup
			|| expected.is_some_and(|expected| expected != &record)
			|| integrity.byte_length > self.max_bytes
		{
			return Err(NativeBackupError::MetadataMismatch);
		}
		let selected = storage.read_file(backup, integrity.byte_length)?;
		if selected.fingerprint().sha256() != &integrity.sha256
			|| selected.bytes().len() != integrity.byte_length
		{
			return Err(NativeBackupError::MetadataMismatch);
		}
		Ok((record, selected, scope.cloned()))
	}
}
