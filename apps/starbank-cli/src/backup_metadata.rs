use std::{
	ffi::OsString,
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use starbank_backup::{BackupIntegrity, BackupReason, BackupRecord, BackupRequest, BankSource};
use starbank_fs::{StorageError, read_snapshot};

use starbank_paths::parse_signing_context;

use crate::{
	bank::SHA256_BYTES,
	failure::{ErrorCode, Failure, Status},
};

const METADATA_VERSION: u32 = 1;
const BACKUP_APPENDIX: &str = ".json";
const METADATA_SUFFIX: &str = ".sc2bank.bak.json";
pub(crate) const MAX_METADATA_BYTES: usize = 4_096;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
	schema_version: u32,
	source_name: String,
	signing_context: Option<BankContext>,
	timestamp_utc: String,
	sequence: u16,
	reason: Reason,
	byte_length: usize,
	sha256: String,
}

impl Metadata {
	pub(crate) fn from_record(record: &BackupRecord) -> Result<Self, StorageError> {
		let source_name = original_name(record.source()).ok_or(StorageError::UnsafePath(
			"the backup source has no portable filename",
		))?;
		let integrity = record.integrity();
		Ok(Self {
			schema_version: METADATA_VERSION,
			source_name: source_name.to_owned(),
			signing_context: context(record.source()),
			timestamp_utc: record.timestamp().to_string(),
			sequence: record.sequence(),
			reason: Reason::from(record.reason()),
			byte_length: integrity.byte_length,
			sha256: hex::encode(integrity.sha256),
		})
	}

	pub(crate) fn read(source: &BankSource, sidecar: &Path) -> Result<BackupRecord, Failure> {
		let bytes = read_snapshot(sidecar, MAX_METADATA_BYTES)?;
		let metadata: Self =
			serde_json::from_slice(bytes.bytes()).map_err(|_| invalid_metadata())?;
		if metadata.schema_version != METADATA_VERSION
			|| Some(metadata.source_name.as_str()) != original_name(source)
			|| metadata.signing_context != context(source)
			|| sidecar.parent() != source.path().parent()
		{
			return Err(invalid_metadata());
		}
		let mut sha256 = [0; SHA256_BYTES];
		hex::decode_to_slice(metadata.sha256, &mut sha256).map_err(|_| invalid_metadata())?;
		let timestamp = metadata
			.timestamp_utc
			.parse()
			.map_err(|_| invalid_metadata())?;
		let record = BackupRecord::from_metadata(
			source.clone(),
			BackupRequest::adjacent(timestamp),
			metadata.sequence,
			metadata.reason.into(),
			BackupIntegrity {
				byte_length: metadata.byte_length,
				sha256,
			},
		)
		.map_err(|_| invalid_metadata())?;
		if sidecar_path(record.id().path()) == sidecar {
			Ok(record)
		} else {
			Err(invalid_metadata())
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reason {
	Requested,
	ExplicitSave,
	BeforeOverwrite,
	BeforeRestore,
	Other,
}

impl From<BackupReason> for Reason {
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

impl From<Reason> for BackupReason {
	fn from(reason: Reason) -> Self {
		match reason {
			Reason::Requested | Reason::Other => Self::Requested,
			Reason::ExplicitSave => Self::ExplicitSave,
			Reason::BeforeOverwrite => Self::BeforeOverwrite,
			Reason::BeforeRestore => Self::BeforeRestore,
		}
	}
}

pub(crate) fn sidecar_path(backup: &Path) -> PathBuf {
	let mut sidecar: OsString = backup.as_os_str().to_owned();
	sidecar.push(BACKUP_APPENDIX);
	sidecar.into()
}

pub(crate) fn is_candidate(path: &Path, source: &BankSource) -> bool {
	let Some(stem) = source.path().file_stem().and_then(|name| name.to_str()) else {
		return false;
	};
	path.file_name()
		.and_then(|name| name.to_str())
		.and_then(|name| name.strip_suffix(METADATA_SUFFIX))
		.and_then(|name| name.strip_prefix(stem))
		.is_some_and(|appendix| appendix.starts_with('_'))
}

fn original_name(source: &BankSource) -> Option<&str> {
	source.path().file_name()?.to_str()
}

fn invalid_metadata() -> Failure {
	Failure::new(
		Status::Validation,
		ErrorCode::InvalidBackup,
		"The backup metadata does not identify this bank and snapshot.",
	)
}
