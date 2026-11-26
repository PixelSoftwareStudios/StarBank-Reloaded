use std::{env, path::Path, process::ExitCode};

use clap::{Parser, error::ErrorKind};

use crate::{
	args::{BankArgs, Cli, Command, JSON_FLAG},
	backup_command,
	bank::{BankSummary, Inspection, SignatureState, read_bank, signing_context},
	diff::BankDiff,
	discovery::discover,
	failure::{ErrorCode, Failure, Status},
	map,
	output::{self, CommandName},
	write_bank::{self, Operation, Target},
};

pub(crate) fn run() -> ExitCode {
	let arguments = env::args_os().collect::<Vec<_>>();
	let json_requested = arguments
		.iter()
		.skip(1)
		.take_while(|argument| *argument != "--")
		.any(|argument| argument == JSON_FLAG);
	let cli = match Cli::try_parse_from(arguments) {
		Ok(cli) => cli,
		Err(error)
			if matches!(
				error.kind(),
				ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
			) || !json_requested =>
		{
			let status = if error.use_stderr() {
				Status::Usage
			} else {
				Status::Success
			};
			return match error.print() {
				Ok(()) => status,
				Err(_) => Status::Io,
			}
			.into();
		}
		Err(_) => {
			return report_failure(
				true,
				&Failure::new(
					Status::Usage,
					ErrorCode::InvalidArguments,
					"Invalid command arguments. Run starbank --help for usage.",
				),
			)
			.into();
		}
	};
	match execute(cli.command, cli.json) {
		Ok(status) => status,
		Err(error) => report_failure(cli.json, &error),
	}
	.into()
}

fn report_failure(json: bool, error: &Failure) -> Status {
	match output::failure(json, error) {
		Ok(()) => error.status,
		Err(_) => Status::Io,
	}
}

fn execute(command: Command, json: bool) -> Result<Status, Failure> {
	match command {
		Command::Inspect(bank) => inspect(bank, json),
		Command::Validate {
			bank,
			require_signature,
		} => validate(bank, require_signature, json),
		Command::Diff { before, after } => diff(&before, &after, json),
		Command::Sign { bank, destination } => {
			let target = Target::amend(destination, &bank.file);
			write_bank::write(bank, target, Operation::Sign, json)
		}
		Command::Edit {
			bank,
			patch,
			destination,
			signing,
		} => {
			let target = Target::amend(destination, &bank.file);
			write_bank::write(bank, target, Operation::Edit(patch, signing), json)
		}
		Command::Export {
			bank,
			output,
			dry_run,
			signing,
		} => {
			let target = Target::export(output, dry_run);
			write_bank::write(bank, target, Operation::Export(signing), json)
		}
		Command::Backup { command } => backup_command::run(command, json),
		Command::Map { command } => map::run(command, json),
		Command::Discover { root } => {
			let discovery = discover(root.as_deref())?;
			let report = discovery.report();
			output::report(json, CommandName::Discover, &report, |out| {
				writeln!(
					out,
					"{} live banks and {} backup files under {:?}",
					report.banks.len(),
					report.backups.len(),
					report.root.display
				)
			})?;
			Ok(Status::Success)
		}
	}
}

fn inspect(args: BankArgs, json: bool) -> Result<Status, Failure> {
	let document = read_bank(&args.file)?;
	let context = signing_context(&args.file, args.context)?;
	let report = Inspection::new(&document, context.as_ref());
	output::report(json, CommandName::Inspect, &report, |out| {
		writeln!(
			out,
			"{} sections, {} keys, {} values. Signature {:?}.",
			report.summary.sections,
			report.summary.keys,
			report.summary.values,
			report.summary.signature.status
		)?;
		for section in &report.contents {
			writeln!(out, "{:?}", section.name)?;
			for key in &section.keys {
				writeln!(out, "  {:?}", key.name)?;
				for value in &key.values {
					writeln!(
						out,
						"    {} {:?} = {:?}",
						value.name, value.kind, value.value
					)?;
				}
			}
		}
		Ok(())
	})?;
	Ok(Status::Success)
}

fn validate(args: BankArgs, require_signature: bool, json: bool) -> Result<Status, Failure> {
	let document = read_bank(&args.file)?;
	let context = signing_context(&args.file, args.context)?;
	let report = BankSummary::new(&document, context.as_ref());
	let status = match report.signature.status {
		SignatureState::Invalid | SignatureState::Malformed => Status::Validation,
		SignatureState::Unsupported if require_signature => Status::Unsupported,
		SignatureState::Valid => Status::Success,
		_ if require_signature => Status::Validation,
		_ => Status::Success,
	};
	output::report(json, CommandName::Validate, &report, |out| {
		writeln!(
			out,
			"Bank XML is valid. Signature {:?}.",
			report.signature.status
		)
	})?;
	Ok(status)
}

fn diff(before: &Path, after: &Path, json: bool) -> Result<Status, Failure> {
	let before = read_bank(before)?;
	let after = read_bank(after)?;
	let report = BankDiff::new(&before, &after);
	output::report(json, CommandName::Diff, &report, |out| {
		if report.source_changed {
			writeln!(
				out,
				"Source changed. {} sections have projected changes. Use --json for exact values.",
				report.sections.len()
			)
		} else {
			writeln!(out, "The banks are byte-identical.")
		}
	})?;
	Ok(Status::Success)
}
