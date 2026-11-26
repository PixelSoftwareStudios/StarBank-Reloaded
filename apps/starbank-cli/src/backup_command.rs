use std::path::Path;

use sc2bank::{Limits, parse};
use serde::Serialize;
use starbank_fs::{
	BackupHealth, BackupPlacement, BackupReasonCode, BackupRecord, BackupVerification, BankSource,
	CommitOutcome, backup_source as bank_source, read_snapshot,
};

use crate::{
	args::BackupCommand,
	backup::catalog,
	bank::SHA256_BYTES,
	discovery::DisplayPath,
	failure::{ErrorCode, Failure, Status},
	output::{self, CommandName},
};

pub(crate) fn run(command: BackupCommand, json: bool) -> Result<Status, Failure> {
	match command {
		BackupCommand::List { file, verify } => list(&bank_source(&file)?, verify, json),
		BackupCommand::Create { file } => create(&bank_source(&file)?, json),
		BackupCommand::Restore {
			file,
			backup,
			expected_current_sha256,
			dry_run,
		} => restore(
			&bank_source(&file)?,
			&std::path::absolute(backup)?,
			&expected_current_sha256,
			dry_run,
			json,
		),
	}
}

fn list(source: &BankSource, verify: bool, json: bool) -> Result<Status, Failure> {
	let limits = Limits::default();
	let mut catalog = catalog(limits)?;
	let verification = if verify {
		BackupVerification::Verify
	} else {
		BackupVerification::Recorded
	};
	let rejected_metadata = catalog.load(source, BackupPlacement::Adjacent, verification)?;
	let report = Listing {
		backups: catalog
			.history(source)
			.into_iter()
			.map(|(record, health)| RecordReport::new(record, health))
			.collect(),
		rejected_metadata,
	};
	output::report(json, CommandName::BackupList, &report, |out| {
		writeln!(out, "{} recorded backups.", report.backups.len())?;
		for backup in &report.backups {
			writeln!(out, "{:?} {:?}", backup.path.display, backup.health)?;
		}
		if report.rejected_metadata > 0 {
			writeln!(
				out,
				"{} metadata files were rejected.",
				report.rejected_metadata
			)?;
		}
		Ok(())
	})?;
	Ok(Status::Success)
}

fn create(source: &BankSource, json: bool) -> Result<Status, Failure> {
	let mut catalog = catalog(Limits::default())?;
	let id = catalog.create(source, BackupPlacement::Adjacent)?;
	let (record, health) = catalog.get(&id).ok_or_else(invalid_backup_path)?;
	let report = RecordReport::new(record, health);
	output::report(json, CommandName::BackupCreate, &report, |out| {
		writeln!(out, "Verified backup at {:?}.", report.path.display)
	})?;
	Ok(Status::Success)
}

fn restore(
	source: &BankSource,
	path: &Path,
	expected_sha256: &str,
	dry_run: bool,
	json: bool,
) -> Result<Status, Failure> {
	let limits = Limits::default();
	let current = read_snapshot(source.path(), limits.document_bytes)?;
	let mut confirmed = [0; SHA256_BYTES];
	hex::decode_to_slice(expected_sha256, &mut confirmed).map_err(|_| {
		Failure::new(
			Status::Validation,
			ErrorCode::InvalidBackup,
			"The current bank fingerprint must contain 64 hexadecimal characters.",
		)
	})?;
	if current.fingerprint().sha256() != &confirmed {
		return Err(Failure::new(
			Status::Conflict,
			ErrorCode::InputChanged,
			"The current bank no longer matches the confirmed fingerprint.",
		));
	}
	if path.parent() != source.path().parent() {
		return Err(invalid_backup_path());
	}
	let mut catalog = catalog(limits)?;
	let prepared = catalog.prepare_restore(source, current, path, BackupPlacement::Adjacent)?;
	parse(prepared.selected().bytes(), limits)?;

	let mut report = RestoreReport {
		snapshot: DisplayPath::new(path),
		before_sha256: hex::encode(confirmed),
		after_sha256: hex::encode(prepared.selected().fingerprint().sha256()),
		reversal_backup: None,
		recovery: None,
		outcome: RestoreOutcome::Prepared,
	};
	let status = if dry_run {
		Status::Success
	} else {
		let outcome = catalog.restore(prepared, BackupPlacement::Adjacent)?;
		let reversal = outcome.backup.map(|backup| backup.path().to_owned());
		report.reversal_backup = reversal.as_ref().map(display);
		match outcome.commit {
			CommitOutcome::CommittedVerified(receipt) => {
				report.outcome = RestoreOutcome::CommittedVerified;
				report.recovery = receipt.recovery().map(|artifact| display(artifact.path()));
				Status::Success
			}
			CommitOutcome::RejectedUnchanged(error) => {
				let failure: Failure = error.into();
				return Err(match reversal {
					Some(path) => failure.with_backup_path(&path),
					None => failure,
				});
			}
			CommitOutcome::CommitUncertain(issue) => {
				report.outcome = RestoreOutcome::CommitUncertain;
				report.recovery = Some(display(issue.recovery().path()));
				Status::Uncertain
			}
		}
	};
	output::report(json, CommandName::BackupRestore, &report, |out| {
		writeln!(
			out,
			"{:?} for {:?}.",
			report.outcome, report.snapshot.display
		)?;
		if let Some(reversal_backup) = &report.reversal_backup {
			writeln!(out, "Reversal backup at {reversal_backup:?}.")?;
		}
		if let Some(recovery) = &report.recovery {
			writeln!(
				out,
				"Inspect recovery location {recovery:?} before retrying."
			)?;
		}
		Ok(())
	})?;
	Ok(status)
}

fn display(path: impl AsRef<Path>) -> String {
	path.as_ref().to_string_lossy().into_owned()
}

fn invalid_backup_path() -> Failure {
	Failure::new(
		Status::Validation,
		ErrorCode::InvalidBackup,
		"Select an adjacent snapshot recorded for this bank.",
	)
}

#[derive(Debug, Serialize)]
struct Listing<'backup> {
	backups: Vec<RecordReport<'backup>>,
	rejected_metadata: usize,
}

#[derive(Debug, Serialize)]
struct RecordReport<'backup> {
	path: DisplayPath<'backup>,
	timestamp_utc: String,
	sequence: u16,
	reason: BackupReasonCode,
	byte_length: usize,
	sha256: String,
	health: Health,
}

impl<'backup> RecordReport<'backup> {
	fn new(record: &'backup BackupRecord, health: BackupHealth) -> Self {
		let integrity = record.integrity();
		Self {
			path: DisplayPath::new(record.id().path()),
			timestamp_utc: record.timestamp().to_string(),
			sequence: record.sequence(),
			reason: BackupReasonCode::from(record.reason()),
			byte_length: integrity.byte_length,
			sha256: hex::encode(integrity.sha256),
			health: match health {
				BackupHealth::Verified => Health::Verified,
				BackupHealth::Missing => Health::Missing,
				BackupHealth::Unverified => Health::Unverified,
			},
		}
	}
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Health {
	Verified,
	Missing,
	Unverified,
}

#[derive(Debug, Serialize)]
struct RestoreReport<'backup> {
	snapshot: DisplayPath<'backup>,
	before_sha256: String,
	after_sha256: String,
	reversal_backup: Option<String>,
	recovery: Option<String>,
	outcome: RestoreOutcome,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum RestoreOutcome {
	Prepared,
	CommittedVerified,
	CommitUncertain,
}
