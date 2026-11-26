#[path = "support/memory.rs"]
mod memory;
#[path = "support/metadata.rs"]
mod metadata;

use memory::{CreateFailure, MemoryStorage, ReplaceFailure};

use std::io::ErrorKind;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use starbank_backup::{
	BackupCatalog, BackupError, BackupHealth, BackupIntegrity, BackupPlacement, BackupReason,
	BackupRecord, BackupRequest, BackupStatus, BackupTimestamp, BankSource, CatalogError,
	CommitState, NamingError, Overwrite, OverwriteBackup, RetentionPolicy,
};

const ORIGINAL: &[u8] = b"<Bank version='1'/>";
const EDITED: &[u8] = b"<Bank version='2'/>";
const NEXT_EDIT: &[u8] = b"<Bank version='3'/>";

fn catalog() -> BackupCatalog {
	BackupCatalog::new(NonZeroUsize::new(1024).unwrap())
}

fn root() -> PathBuf {
	std::env::temp_dir().join("starbank-mock-backups")
}

fn source() -> BankSource {
	BankSource::new(root().join("Player.Data.SC2Bank")).unwrap()
}

fn stamp() -> BackupTimestamp {
	"20261126_193800".parse().unwrap()
}

fn request() -> BackupRequest<'static> {
	BackupRequest::adjacent(stamp())
}

fn overwrite(
	catalog: &mut BackupCatalog,
	storage: &mut MemoryStorage,
	bank: &BankSource,
	expected: u64,
	bytes: &[u8],
	backup: OverwriteBackup<'_>,
) -> starbank_backup::StorageOutcome<MemoryStorage> {
	catalog.overwrite(
		storage,
		bank,
		Overwrite {
			expected: &expected,
			bytes,
			backup,
		},
	)
}

#[test]
fn only_portable_bank_names_and_real_utc_dates_are_accepted() {
	for invalid in [
		"00000101_000000",
		"20010229_123456",
		"19000229_123456",
		"20260431_000000",
		"20261301_000000",
		"20260600_000000",
		"20260601_240000",
		"20260601_126000",
		"20260601_125960",
		"2026-06-01T12:00:00Z",
		"202606é_000000",
	] {
		assert_eq!(
			invalid.parse::<BackupTimestamp>(),
			Err(NamingError::Timestamp)
		);
	}
	assert_eq!(
		"20000229_235959"
			.parse::<BackupTimestamp>()
			.unwrap()
			.to_string(),
		"20000229_235959"
	);
	assert!(matches!(
		BankSource::new(PathBuf::from("relative.SC2Bank")),
		Err(NamingError::Location)
	));
	assert!(matches!(
		BankSource::new(root().join("..").join("escape.SC2Bank")),
		Err(NamingError::Location)
	));
	for invalid in [
		"Game.SC2Bank.bak",
		"bank.txt",
		"with:colon.SC2Bank",
		".sc2bank",
	] {
		assert_eq!(
			BankSource::new(root().join(invalid)),
			Err(NamingError::BankName)
		);
	}
	for filename in ["München.SC2Bank", "My bank.sc2bank", "with.dots.SC2BANK"] {
		assert!(BankSource::new(root().join(filename)).is_ok());
	}
}

#[test]
fn collision_preserves_existing_file_and_catalogued_backup() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let bank = source();
	storage.put(bank.path(), ORIGINAL);
	let existing = root().join("Player.Data_20261126_193800.sc2bank.bak");
	storage.put(&existing, b"preexisting with no catalog record");

	let first = catalog.create(&mut storage, &bank, request()).unwrap();
	let second = catalog.create(&mut storage, &bank, request()).unwrap();
	assert_eq!(
		first.path(),
		root().join("Player.Data_20261126_193800_1.sc2bank.bak")
	);
	assert_eq!(
		second.path(),
		root().join("Player.Data_20261126_193800_2.sc2bank.bak")
	);
	assert_eq!(
		storage.bytes(&existing),
		b"preexisting with no catalog record"
	);
	assert_eq!(storage.bytes(first.path()), ORIGINAL);
	assert_eq!(storage.bytes(second.path()), ORIGINAL);
	assert_eq!(
		&storage.writes,
		&[
			existing.clone(),
			first.path().to_owned(),
			existing,
			second.path().to_owned()
		]
	);
}

#[test]
fn preserves_extension_and_scope_and_rejects_long_generated_filename() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let long_bank = BankSource::new(root().join(format!("{}.SC2Bank", "b".repeat(241)))).unwrap();
	storage.put(long_bank.path(), ORIGINAL);
	assert!(matches!(
		catalog.create(&mut storage, &long_bank, request()),
		Err(BackupError::Naming(NamingError::NameTooLong))
	));
	let bank = source();
	storage.put(bank.path(), ORIGINAL);
	assert!(matches!(
		catalog.create(
			&mut storage,
			&bank,
			BackupRequest {
				timestamp: stamp(),
				placement: BackupPlacement::Managed(Path::new("relative")),
			}
		),
		Err(BackupError::Naming(NamingError::Location))
	));
	assert!(storage.writes.is_empty());
}

#[test]
fn first_overwrite_is_protected_once_per_session_and_explicit_remains_adjacent() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let bank = source();
	let initial_fingerprint = storage.put(bank.path(), ORIGINAL);
	let managed_dir = root().join("managed");
	let backup = OverwriteBackup::FirstInSession(BackupRequest {
		timestamp: stamp(),
		placement: BackupPlacement::Managed(&managed_dir),
	});

	let first = overwrite(
		&mut catalog,
		&mut storage,
		&bank,
		initial_fingerprint,
		EDITED,
		backup,
	)
	.unwrap();
	let first_id = first.backup.unwrap();
	assert!(matches!(first.commit, CommitState::Verified(_)));
	assert_eq!(first_id.path().parent(), Some(managed_dir.as_path()));
	assert_eq!(storage.bytes(first_id.path()), ORIGINAL);
	assert_eq!(
		storage.records.get(&first_id),
		catalog.get(&first_id).map(|(r, _)| r)
	);
	assert_eq!(storage.bytes(bank.path()), EDITED);
	let expected = storage.current(&bank);
	assert!(
		overwrite(
			&mut catalog,
			&mut storage,
			&bank,
			expected,
			NEXT_EDIT,
			backup
		)
		.unwrap()
		.backup
		.is_none()
	);

	let expected = storage.current(&bank);
	let explicit = overwrite(
		&mut catalog,
		&mut storage,
		&bank,
		expected,
		EDITED,
		OverwriteBackup::Explicit(stamp()),
	)
	.unwrap()
	.backup
	.unwrap();
	assert_eq!(explicit.path().parent(), bank.path().parent());
	assert_eq!(storage.bytes(explicit.path()), NEXT_EDIT);
	assert_eq!(
		catalog.get(&explicit).unwrap().0.reason(),
		BackupReason::ExplicitSave
	);

	let expected = storage.current(&bank);
	let mut new_session = BackupCatalog::new(NonZeroUsize::new(1024).unwrap());
	new_session
		.import(catalog.get(&first_id).unwrap().0.clone())
		.unwrap();
	assert!(
		overwrite(
			&mut new_session,
			&mut storage,
			&bank,
			expected,
			NEXT_EDIT,
			backup
		)
		.unwrap()
		.backup
		.is_some()
	);
	assert_eq!(storage.bytes(first_id.path()), ORIGINAL);
}

#[test]
fn failure_or_uncertainty_on_backup_never_reaches_live_write() {
	for failure in [
		CreateFailure::Rejected,
		CreateFailure::PublishedButUncertain,
	] {
		let mut storage = MemoryStorage::default();
		let mut catalog = catalog();
		let bank = source();
		let initial = storage.put(bank.path(), ORIGINAL);
		storage.create_failure = Some(failure);
		let error = overwrite(
			&mut catalog,
			&mut storage,
			&bank,
			initial,
			EDITED,
			OverwriteBackup::FirstInSession(request()),
		)
		.unwrap_err();
		match error {
			BackupError::Storage(error) => assert_eq!(error.kind(), ErrorKind::PermissionDenied),
			BackupError::UncertainBackup { path, issue } => {
				assert_eq!(issue, "create readback failed");
				assert_eq!(storage.bytes(&path), ORIGINAL);
			}
			unexpected => panic!("unexpected backup outcome: {unexpected:?}"),
		}
		assert_eq!(catalog.status(&bank), BackupStatus::NeverBackedUp);
		assert_eq!(storage.bytes(bank.path()), ORIGINAL);
		assert!(!storage.writes.contains(&bank.path().to_owned()));
	}
}

#[test]
fn stale_target_never_uses_stale_open_bytes_and_first_policy_remains_pending() {
	let mut storage = MemoryStorage::default();
	let mut catalog = catalog();
	let bank = source();
	let stale = storage.put(bank.path(), ORIGINAL);
	let current = storage.put(bank.path(), EDITED);
	let mode = OverwriteBackup::FirstInSession(request());
	assert!(matches!(
		overwrite(&mut catalog, &mut storage, &bank, stale, NEXT_EDIT, mode),
		Err(BackupError::TargetChanged)
	));
	assert!(storage.writes.is_empty());

	storage.replace_failure = Some(ReplaceFailure::ExternalChange);
	let attempt = overwrite(&mut catalog, &mut storage, &bank, current, NEXT_EDIT, mode).unwrap();
	assert!(matches!(attempt.commit, CommitState::Rejected(_)));
	assert_eq!(storage.bytes(attempt.backup.unwrap().path()), EDITED);
	assert_eq!(storage.bytes(bank.path()), b"<External/>");

	let expected = storage.current(&bank);
	let retry = overwrite(&mut catalog, &mut storage, &bank, expected, NEXT_EDIT, mode).unwrap();
	assert_eq!(storage.bytes(retry.backup.unwrap().path()), b"<External/>");
	assert!(matches!(retry.commit, CommitState::Verified(_)));
}

#[test]
fn restore_keeps_exact_bytes_and_a_reversible_snapshot() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let bank = source();
	storage.put(bank.path(), ORIGINAL);
	let first = catalog.create(&mut storage, &bank, request()).unwrap();
	let previous = storage.put(bank.path(), EDITED);

	let restored = catalog
		.restore(&mut storage, &first, &bank, &previous, request())
		.unwrap();
	assert!(matches!(restored.commit, CommitState::Verified(_)));
	assert_eq!(storage.bytes(bank.path()), ORIGINAL);

	let reversing_backup = restored.backup.unwrap();
	assert_eq!(storage.bytes(reversing_backup.path()), EDITED);
	assert_eq!(storage.bytes(first.path()), ORIGINAL);
	assert_eq!(
		catalog.get(&reversing_backup).unwrap().0.reason(),
		BackupReason::BeforeRestore
	);

	let current = storage.current(&bank);
	let second = catalog
		.restore(&mut storage, &reversing_backup, &bank, &current, request())
		.unwrap();
	assert_eq!(storage.bytes(bank.path()), EDITED);
	assert_eq!(storage.bytes(second.backup.unwrap().path()), ORIGINAL);
	assert_eq!(catalog.history(&bank).len(), 3);
}

#[test]
fn corrupt_or_missing_backup_and_wrong_scope_reject_restore() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let bank = source();
	let current = storage.put(bank.path(), ORIGINAL);
	let first = catalog.create(&mut storage, &bank, request()).unwrap();
	let other = BankSource::new(root().join("Other.SC2Bank")).unwrap();
	assert!(matches!(
		catalog.restore(&mut storage, &first, &other, &current, request()),
		Err(BackupError::WrongSource)
	));

	storage.put(first.path(), EDITED);
	assert_eq!(ORIGINAL.len(), EDITED.len());
	assert!(matches!(
		catalog.restore(&mut storage, &first, &bank, &current, request()),
		Err(BackupError::IntegrityMismatch)
	));
	assert_eq!(
		catalog.status(&bank),
		BackupStatus::Latest(BackupHealth::Unverified)
	);

	storage.files.remove(first.path());
	assert!(matches!(
		catalog.verify(&mut storage, &first),
		Err(BackupError::MissingFile)
	));
	assert_eq!(
		catalog.status(&bank),
		BackupStatus::Latest(BackupHealth::Missing)
	);
	assert_eq!(storage.writes, vec![first.path().to_owned()]);
	assert_eq!(storage.bytes(bank.path()), ORIGINAL);
}

#[test]
fn restore_uncertainty_keeps_the_verified_reversal_snapshot() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let bank = source();
	storage.put(bank.path(), ORIGINAL);
	let first = catalog.create(&mut storage, &bank, request()).unwrap();
	let current = storage.put(bank.path(), EDITED);
	storage.replace_failure = Some(ReplaceFailure::PublishedButUncertain);
	let uncertain = catalog
		.restore(&mut storage, &first, &bank, &current, request())
		.unwrap();
	assert!(matches!(
		uncertain.commit,
		CommitState::Uncertain("replace readback failed")
	));
	assert_eq!(storage.bytes(uncertain.backup.unwrap().path()), EDITED);
	assert_eq!(storage.bytes(first.path()), ORIGINAL);
}

#[test]
fn imported_metadata_requires_fresh_verification_and_retention_is_advisory() {
	let mut catalog = catalog();
	let mut storage = MemoryStorage::default();
	let bank = source();
	storage.put(bank.path(), ORIGINAL);
	let imported = BackupRecord::from_metadata(
		bank.clone(),
		request(),
		0,
		BackupReason::Requested,
		BackupIntegrity::measure(ORIGINAL),
	)
	.unwrap();
	let imported_id = imported.id().clone();
	storage.put(imported.id().path(), ORIGINAL);
	catalog.import(imported.clone()).unwrap();
	assert_eq!(catalog.import(imported), Err(CatalogError::DuplicateId));
	assert_eq!(
		catalog.status(&bank),
		BackupStatus::Latest(BackupHealth::Unverified)
	);
	catalog.verify(&mut storage, &imported_id).unwrap();
	assert_eq!(
		catalog.status(&bank),
		BackupStatus::Latest(BackupHealth::Verified)
	);

	let second = catalog.create(&mut storage, &bank, request()).unwrap();
	let older = catalog.retention_candidates(&bank, RetentionPolicy::KeepNewest(NonZeroUsize::MIN));
	assert_eq!(older.len(), 1);
	assert_eq!(older[0].id(), &imported_id);
	assert_eq!(storage.bytes(older[0].id().path()), ORIGINAL);
	assert_eq!(catalog.history(&bank)[0].0.id(), &second);
	assert!(
		catalog
			.retention_candidates(&bank, RetentionPolicy::KeepAll)
			.is_empty()
	);
}

#[test]
fn supplies_sha256_length_evidence_and_rejects_oversize_content() {
	let digest = BackupIntegrity::measure(b"abc");
	assert_eq!(digest.byte_length, 3);
	assert_eq!(
		digest.sha256,
		[
			0xBA, 0x78, 0x16, 0xBF, 0x8F, 0x01, 0xCF, 0xEA, 0x41, 0x41, 0x40, 0xDE, 0x5D, 0xAE,
			0x22, 0x23, 0xB0, 0x03, 0x61, 0xA3, 0x96, 0x17, 0x7A, 0x9C, 0xB4, 0x10, 0xFF, 0x61,
			0xF2, 0x00, 0x15, 0xAD,
		]
	);
	let mut catalog = BackupCatalog::new(NonZeroUsize::new(3).unwrap());
	let mut storage = MemoryStorage::default();
	let bank = source();
	let current = storage.put(bank.path(), b"abc");
	assert!(matches!(
		overwrite(
			&mut catalog,
			&mut storage,
			&bank,
			current,
			b"long",
			OverwriteBackup::Disabled
		),
		Err(BackupError::Catalog(CatalogError::SnapshotTooLarge))
	));
	assert!(storage.writes.is_empty());
}
