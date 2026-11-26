use std::{
	ffi::OsString,
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starbank_backup::{
	BackupIntegrity, BackupPlacement, BackupReason, BackupRecord, BackupRequest, BankSource,
};
use starbank_paths::parse_signing_context;

use crate::{NativeBackupError, StorageError, read_snapshot};

const ADJACENT_VERSION: u32 = 1;
const MANAGED_VERSION: u32 = 2;
const BACKUP_APPENDIX: &str = ".json";
const METADATA_SUFFIX: &str = ".sc2bank.bak.json";
const SHA256_BYTES: usize = 32;
pub(super) const MAX_METADATA_BYTES: usize = 4_096;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Metadata {
	schema_version: u32,
	source_name: String,
	signing_context: Option<BankContext>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	source_path_sha256: Option<String>,
	timestamp_utc: String,
	sequence: u16,
	reason: BackupReasonCode,
	byte_length: usize,
	sha256: String,
}

impl Metadata {
	pub(super) fn from_record(record: &BackupRecord) -> Result<Self, StorageError> {
		let source_name = original_name(record.source()).ok_or(StorageError::UnsafePath(
			"the backup source has no portable filename",
		))?;
		let integrity = record.integrity();
		let adjacent = record.directory() == record.source().path().parent();
		Ok(Self {
			schema_version: if adjacent {
				ADJACENT_VERSION
			} else {
				MANAGED_VERSION
			},
			source_name: source_name.to_owned(),
			signing_context: context(record.source()),
			source_path_sha256: (!adjacent).then(|| source_scope(record.source())),
			timestamp_utc: record.timestamp().to_string(),
			sequence: record.sequence(),
			reason: BackupReasonCode::from(record.reason()),
			byte_length: integrity.byte_length,
			sha256: hex::encode(integrity.sha256),
		})
	}

	pub(super) fn read(
		source: &BankSource,
		placement: BackupPlacement<'_>,
		sidecar: &Path,
	) -> Result<Option<BackupRecord>, NativeBackupError> {
		let directory = backup_directory(source, placement)?;
		if sidecar.parent() != Some(directory) {
			return Err(NativeBackupError::MetadataMismatch);
		}
		let metadata: Self =
			serde_json::from_slice(read_snapshot(sidecar, MAX_METADATA_BYTES)?.bytes())
				.map_err(|_| NativeBackupError::MetadataMismatch)?;
		let adjacent = source.path().parent() == Some(directory);
		match (
			adjacent,
			metadata.schema_version,
			&metadata.source_path_sha256,
		) {
			(true, ADJACENT_VERSION, None) => {}
			(false, MANAGED_VERSION, Some(scope)) => {
				let mut scope_bytes = [0; SHA256_BYTES];
				hex::decode_to_slice(scope, &mut scope_bytes)
					.map_err(|_| NativeBackupError::MetadataMismatch)?;
				if &source_scope(source) != scope {
					return Ok(None);
				}
			}
			_ => return Err(NativeBackupError::MetadataMismatch),
		}
		if Some(metadata.source_name.as_str()) != original_name(source)
			|| metadata.signing_context != context(source)
		{
			return Err(NativeBackupError::MetadataMismatch);
		}
		let mut sha256 = [0; SHA256_BYTES];
		hex::decode_to_slice(metadata.sha256, &mut sha256)
			.map_err(|_| NativeBackupError::MetadataMismatch)?;
		let timestamp = metadata
			.timestamp_utc
			.parse()
			.map_err(|_| NativeBackupError::MetadataMismatch)?;
		let record = BackupRecord::from_metadata(
			source.clone(),
			BackupRequest {
				timestamp,
				placement,
			},
			metadata.sequence,
			metadata.reason.into(),
			BackupIntegrity {
				byte_length: metadata.byte_length,
				sha256,
			},
		)
		.map_err(|_| NativeBackupError::MetadataMismatch)?;
		if sidecar_path(record.id().path()) == sidecar {
			Ok(Some(record))
		} else {
			Err(NativeBackupError::MetadataMismatch)
		}
	}
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BankContext {
	author: String,
	player: String,
	bank_name: String,
}

fn context(source: &BankSource) -> Option<BankContext> {
	parse_signing_context(source.path()).map(|hint| BankContext {
		author: hint.author,
		player: hint.player,
		bank_name: hint.bank_name,
	})
}

fn source_scope(source: &BankSource) -> String {
	// Persist native code units rather than relying on Rust's unspecified OsStr encoding.
	let path = source.path().as_os_str();
	let mut digest = Sha256::new();
	#[cfg(unix)]
	{
		use std::os::unix::ffi::OsStrExt;

		digest.update(b"unix\0");
		digest.update(path.as_bytes());
	}
	#[cfg(windows)]
	{
		use std::os::windows::ffi::OsStrExt;

		digest.update(b"win\0");
		for unit in path.encode_wide() {
			digest.update(unit.to_le_bytes());
		}
	}
	#[cfg(not(any(unix, windows)))]
	digest.update(path.as_encoded_bytes());
	hex::encode(digest.finalize())
}

/// Stable reason labels used in versioned sidecars and command reports.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupReasonCode {
	/// User-requested snapshot outside save or restore.
	Requested,
	/// Explicit adjacent snapshot during save.
	ExplicitSave,
	/// Automatic snapshot before an overwrite.
	BeforeOverwrite,
	/// Snapshot taken so a restore can be reversed.
	BeforeRestore,
	/// Reserved report label for reasons outside this version.
	Other,
}

impl From<BackupReason> for BackupReasonCode {
	fn from(reason: BackupReason) -> Self {
		match reason {
			BackupReason::Requested => Self::Requested,
			BackupReason::ExplicitSave => Self::ExplicitSave,
			BackupReason::BeforeOverwrite => Self::BeforeOverwrite,
			BackupReason::BeforeRestore => Self::BeforeRestore,
			_ => Self::Other,
		}
	}
}

impl From<BackupReasonCode> for BackupReason {
	fn from(reason: BackupReasonCode) -> Self {
		match reason {
			BackupReasonCode::Requested | BackupReasonCode::Other => Self::Requested,
			BackupReasonCode::ExplicitSave => Self::ExplicitSave,
			BackupReasonCode::BeforeOverwrite => Self::BeforeOverwrite,
			BackupReasonCode::BeforeRestore => Self::BeforeRestore,
		}
	}
}

pub(super) fn sidecar_path(backup: &Path) -> PathBuf {
	let mut sidecar: OsString = backup.as_os_str().to_owned();
	sidecar.push(BACKUP_APPENDIX);
	sidecar.into()
}

pub(super) fn is_candidate(path: &Path, source: &BankSource) -> bool {
	let Some(stem) = source.path().file_stem().and_then(|name| name.to_str()) else {
		return false;
	};
	path.file_name()
		.and_then(|name| name.to_str())
		.and_then(|name| name.strip_suffix(METADATA_SUFFIX))
		.and_then(|name| name.strip_prefix(stem))
		.is_some_and(|appendix| appendix.starts_with('_'))
}

pub(super) fn backup_directory<'a>(
	source: &'a BankSource,
	placement: BackupPlacement<'a>,
) -> Result<&'a Path, NativeBackupError> {
	let directory = match placement {
		BackupPlacement::Adjacent => source
			.path()
			.parent()
			.ok_or(NativeBackupError::InvalidSource)?,
		BackupPlacement::Managed(directory) => directory,
	};
	if !directory.is_absolute()
		|| directory
			.components()
			.any(|component| component == std::path::Component::ParentDir)
	{
		return Err(NativeBackupError::InvalidLocation);
	}
	Ok(directory)
}

fn original_name(source: &BankSource) -> Option<&str> {
	source.path().file_name()?.to_str()
}
