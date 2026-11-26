use std::path::{Path, PathBuf};

use sc2bank::{Limits, parse};
use serde::Serialize;
use starbank_editor::{ExportSigning, Session};
use starbank_fs::{CommitIntent, CommitOutcome, Fingerprint, commit};

use crate::{
	args::{BankArgs, DestinationArgs, OptionalSigningArgs},
	backup,
	bank::{SignatureState, invalid_context, signing_context},
	diff::BankDiff,
	discovery::DisplayPath,
	failure::{ErrorCode, Failure, Status},
	input,
	output::{self, CommandName},
	patch::Patch,
};

pub(crate) struct Target {
	path: Option<PathBuf>,
	overwrite: Option<BackupPreference>,
	dry_run: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum BackupPreference {
	Automatic,
	Requested,
}

impl Target {
	pub(crate) fn amend(args: DestinationArgs, input: &Path) -> Self {
		Self {
			path: args
				.output
				.or_else(|| args.in_place.then(|| input.to_owned())),
			overwrite: args.in_place.then_some(if args.backup {
				BackupPreference::Requested
			} else {
				BackupPreference::Automatic
			}),
			dry_run: args.dry_run,
		}
	}

	pub(crate) fn export(path: PathBuf, dry_run: bool) -> Self {
		Self {
			path: Some(path),
			overwrite: None,
			dry_run,
		}
	}
}

pub(crate) enum Operation {
	Sign,
	Edit(PathBuf, OptionalSigningArgs),
	Export(OptionalSigningArgs),
}

impl Operation {
	fn command_name(&self) -> CommandName {
		match self {
			Self::Sign => CommandName::Sign,
			Self::Edit(..) => CommandName::Edit,
			Self::Export(_) => CommandName::Export,
		}
	}

	fn signing_args(&self) -> Option<&OptionalSigningArgs> {
		match self {
			Self::Sign => None,
			Self::Edit(_, args) | Self::Export(args) => Some(args),
		}
	}
}

pub(crate) fn write(
	bank: BankArgs,
	target: Target,
	operation: Operation,
	json: bool,
) -> Result<Status, Failure> {
	let limits = Limits::default();
	let source = input::read(&bank.file, limits.document_bytes)?;
	let context_path = target.path.as_deref().unwrap_or(&bank.file);
	let context = signing_context(context_path, bank.context)?;
	let mut session = Session::open(source.bytes(), limits, context.clone())?;
	if let Operation::Edit(path, _) = &operation {
		Patch::read(path)?.apply(&mut session, source.fingerprint().sha256())?;
	}

	let flags = operation.signing_args();
	let policy = match (flags, context.as_ref()) {
		(None, context) => ExportSigning::Sign(context.ok_or_else(invalid_context)?),
		(Some(options), Some(context)) if !options.no_resign => ExportSigning::Sign(context),
		_ => ExportSigning::Preserve,
	};
	let prepared = session.prepare(session.revision(), policy)?;
	let status = SignatureState::from(prepared.signature_status());
	let needs_confirmation = matches!(
		status,
		SignatureState::Malformed
			| SignatureState::MissingContext
			| SignatureState::Unsupported
			| SignatureState::Invalid
	) && !flags.is_some_and(|options| options.allow_stale_signature);
	let before = parse(source.bytes(), limits)?;
	let after = parse(prepared.bytes(), limits)?;
	let diff = BankDiff::new(&before, &after);
	let no_overwrite_needed = target.overwrite.is_some() && !diff.source_changed;
	let mut report = WriteReport {
		destination: target.path.as_deref().map(DisplayPath::new),
		revision: prepared.revision().to_string(),
		signature: status,
		diff,
		outcome: Outcome::Prepared,
		requires_signature_confirmation: needs_confirmation,
		backup: None,
		recovery: None,
	};

	let status = if target.dry_run {
		Status::Success
	} else if no_overwrite_needed {
		report.outcome = Outcome::Unchanged;
		Status::Success
	} else if needs_confirmation {
		return Err(Failure::new(
			Status::Validation,
			ErrorCode::StaleSignature,
			"The prepared signature needs verification or explicit --allow-stale-signature.",
		));
	} else {
		publish(
			&target,
			prepared.bytes(),
			source.fingerprint(),
			limits,
			&mut report,
		)?
	};

	output::report(json, operation.command_name(), &report, |out| {
		writeln!(
			out,
			"{:?}. Signature {:?}. {} projected section changes.",
			report.outcome,
			report.signature,
			report.diff.sections.len()
		)?;
		if report.requires_signature_confirmation {
			writeln!(out, "Writing these bytes requires signature confirmation.")?;
		}
		if let Some(path) = &report.recovery {
			writeln!(
				out,
				"Recovery location: {:?}. Inspect it before retrying.",
				path
			)?;
		}
		Ok(())
	})?;
	Ok(status)
}

fn publish(
	target: &Target,
	bytes: &[u8],
	expected: &Fingerprint,
	limits: Limits,
	report: &mut WriteReport<'_>,
) -> Result<Status, Failure> {
	let path = target.path.as_ref().ok_or_else(|| {
		Failure::new(
			Status::Usage,
			ErrorCode::InvalidArguments,
			"Choose --output, --in-place or --dry-run.",
		)
	})?;
	if let Some(preference) = target.overwrite {
		let effect = backup::overwrite(path, expected, bytes, preference, limits)?;
		report.backup = effect
			.backup
			.as_deref()
			.map(|path| path.to_string_lossy().into_owned());
		return apply_effect(effect.commit, report).map_err(|failure| match effect.backup {
			Some(path) => failure.with_backup_path(&path),
			None => failure,
		});
	}
	apply_effect(
		commit(path, bytes, CommitIntent::CreateNew, limits.document_bytes),
		report,
	)
}

fn apply_effect(effect: CommitOutcome, report: &mut WriteReport<'_>) -> Result<Status, Failure> {
	match effect {
		CommitOutcome::CommittedVerified(receipt) => {
			report.outcome = Outcome::CommittedVerified;
			report.recovery = receipt
				.recovery()
				.map(|artifact| artifact.path().to_string_lossy().into_owned());
			Ok(Status::Success)
		}
		CommitOutcome::RejectedUnchanged(error) => Err(error.into()),
		CommitOutcome::CommitUncertain(issue) => {
			report.outcome = Outcome::CommitUncertain;
			report.recovery = Some(issue.recovery().path().to_string_lossy().into_owned());
			Ok(Status::Uncertain)
		}
	}
}

#[derive(Debug, Serialize)]
struct WriteReport<'source> {
	destination: Option<DisplayPath<'source>>,
	revision: String,
	signature: SignatureState,
	diff: BankDiff<'source>,
	outcome: Outcome,
	requires_signature_confirmation: bool,
	backup: Option<String>,
	recovery: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
	Prepared,
	Unchanged,
	CommittedVerified,
	CommitUncertain,
}
