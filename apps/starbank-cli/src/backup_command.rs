use std::path::Path;

use sc2bank::{Limits, parse};
use serde::Serialize;
use starbank_fs::{
	BackupHealth, BackupPlacement, BackupReasonCode, BackupRecord, BackupVerification, BankSource,
	CommitOutcome, backup_source as bank_source, check_replace_scope, read_snapshot,
};

use crate::{
	args::BackupCommand,
	backup::catalog,
	bank::{SHA256_BYTES, SignatureReport, signing_context},
	diff::BankDiff,
	discovery::DisplayPath,
	failure::{ErrorCode, Failure, Status},
	output::{self, CommandName, Publication},
};

pub(crate) fn run(command: BackupCommand, json: bool) -> Result<Status, Failure> {
	match command {
		BackupCommand::List { file, verify } => list(&bank_source(&file)?, verify, json),
		BackupCommand::Create { file } => create(&bank_source(&file)?, json),
		BackupCommand::Restore {
			file,
			backup,
			expected_current_sha256,
			expected_backup_sha256,
			dry_run,
		} => restore(
			&bank_source(&file)?,
			&std::path::absolute(backup)?,
			&expected_current_sha256,
			expected_backup_sha256.as_deref(),
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
	let delivery = output::report(json, CommandName::BackupCreate, &report, |out| {
		writeln!(out, "Verified backup at {:?}.", report.path.display)
	});
	Ok(output::finish(
		delivery,
		Status::Success,
		Publication::Possible,
	))
}

fn restore(
	source: &BankSource,
	path: &Path,
	expected_current_sha256: &str,
	expected_backup_sha256: Option<&str>,
	dry_run: bool,
	json: bool,
) -> Result<Status, Failure> {
	let limits = Limits::default();
	let current = read_snapshot(source.path(), limits.document_bytes)?;
	let confirmed = decode_digest(
		expected_current_sha256,
		"The current bank fingerprint must contain 64 hexadecimal characters.",
	)?;
	if current.fingerprint().sha256() != &confirmed {
		return Err(Failure::new(
			Status::Conflict,
			ErrorCode::InputChanged,
			"The current bank no longer matches the confirmed fingerprint.",
		));
	}
	let expected_backup = expected_backup_sha256
		.map(|digest| {
			decode_digest(
				digest,
				"The backup fingerprint must contain 64 hexadecimal characters.",
			)
		})
		.transpose()?;
	if path.parent() != source.path().parent() {
		return Err(invalid_backup_path());
	}
	let mut catalog = catalog(limits)?;
	let prepared = catalog.prepare_restore(source, current, path, BackupPlacement::Adjacent)?;
	let selected_digest = prepared.selected().fingerprint().sha256();
	if expected_backup
		.as_ref()
		.is_some_and(|expected| expected != selected_digest)
	{
		return Err(Failure::new(
			Status::Conflict,
			ErrorCode::BackupChanged,
			"The selected backup no longer matches the confirmed fingerprint.",
		));
	}
	let selected_document = parse(prepared.selected().bytes(), limits)?;
	let current_document = parse(prepared.current().bytes(), limits).map_err(Failure::from);
	// Restore reports signature verification when the destination provides a usable context.
	let context = signing_context(source.path(), Default::default()).unwrap_or_default();
	let signature = SignatureReport::new(&selected_document, context.as_ref());
	let diff = current_document
		.as_ref()
		.ok()
		.map(|current| BankDiff::new(current, &selected_document));

	let mut report = RestoreReport {
		snapshot: DisplayPath::new(path),
		before_sha256: hex::encode(confirmed),
		after_sha256: hex::encode(selected_digest),
		diff,
		comparison_diagnostic: current_document.as_ref().err(),
		signature,
		reversal_backup: None,
		recovery: None,
		outcome: RestoreOutcome::Prepared,
	};
	let status = if dry_run {
		Status::Success
	} else {
		check_replace_scope(source.path())?;
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
	let publication = match report.outcome {
		RestoreOutcome::Prepared => Publication::None,
		RestoreOutcome::CommittedVerified | RestoreOutcome::CommitUncertain => {
			Publication::Possible
		}
	};
	let delivery = output::report(json, CommandName::BackupRestore, &report, |out| {
		writeln!(
			out,
			"{:?} for {:?}. Signature {:?}.",
			report.outcome, report.snapshot.display, report.signature.status
		)?;
		if let Some(diff) = &report.diff {
			if diff.source_changed {
				writeln!(
					out,
					"Bank bytes differ. {} sections have projected changes. Use --json for exact values.",
					diff.sections.len()
				)?;
			} else {
				writeln!(out, "The backup and live bank are byte-identical.")?;
			}
		} else if let Some(diagnostic) = report.comparison_diagnostic {
			writeln!(out, "Comparison unavailable. {diagnostic}")?;
		}
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
	});
	Ok(output::finish(delivery, status, publication))
}

fn decode_digest(value: &str, message: &'static str) -> Result<[u8; SHA256_BYTES], Failure> {
	let mut digest = [0; SHA256_BYTES];
	hex::decode_to_slice(value, &mut digest)
		.map_err(|_| Failure::new(Status::Validation, ErrorCode::InvalidBackup, message))?;
	Ok(digest)
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
	diff: Option<BankDiff<'backup>>,
	comparison_diagnostic: Option<&'backup Failure>,
	signature: SignatureReport,
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
