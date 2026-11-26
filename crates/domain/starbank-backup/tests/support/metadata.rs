use std::io::ErrorKind;

use starbank_backup::{BackupError, BackupIntegrity, BackupStatus, OverwriteBackup, RecordFailure};

use super::{EDITED, MemoryStorage, ORIGINAL, catalog, overwrite, request, source};

#[test]
fn metadata_failure_retains_snapshot_and_stops_before_live_write() {
	for failure in [
		RecordFailure::Rejected(ErrorKind::PermissionDenied.into()),
		RecordFailure::Uncertain("metadata sync failed"),
	] {
		let mut storage = MemoryStorage::default();
		let mut catalog = catalog();
		let bank = source();
		let initial = storage.put(bank.path(), ORIGINAL);
		storage.record_failure = Some(failure);
		let error = overwrite(
			&mut catalog,
			&mut storage,
			&bank,
			initial,
			EDITED,
			OverwriteBackup::FirstInSession(request()),
		)
		.unwrap_err();
		let BackupError::Metadata { record, .. } = error else {
			panic!("unexpected backup outcome: {error:?}");
		};

		assert_eq!(record.source(), &bank);
		assert_eq!(storage.bytes(record.id().path()), ORIGINAL);
		assert_eq!(storage.bytes(bank.path()), ORIGINAL);
		assert_eq!(record.integrity(), BackupIntegrity::measure(ORIGINAL));
		assert_eq!(catalog.status(&bank), BackupStatus::NeverBackedUp);
		assert_eq!(storage.writes, vec![record.id().path().to_owned()]);
	}
}
