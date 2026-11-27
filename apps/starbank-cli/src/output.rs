use std::io::{self, Write};

use serde::Serialize;

use crate::failure::{Failure, Status};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandName {
	Inspect,
	Validate,
	Diff,
	Discover,
	MapInspect,
	MapScan,
	Sign,
	Edit,
	Export,
	BackupList,
	BackupCreate,
	BackupRestore,
}

/// Whether the command may have published a destination or auxiliary file.
#[derive(Clone, Copy)]
pub(crate) enum Publication {
	None,
	Possible,
}

#[derive(Serialize)]
struct Report<'report, T> {
	schema_version: u32,
	command: CommandName,
	result: &'report T,
}

#[derive(Serialize)]
struct ErrorReport<'failure> {
	schema_version: u32,
	error: &'failure Failure,
}

pub(crate) fn report<T: Serialize>(
	json: bool,
	command: CommandName,
	value: &T,
	human: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
	let mut stdout = io::stdout().lock();
	if json {
		write_json(
			&mut stdout,
			&Report {
				schema_version: SCHEMA_VERSION,
				command,
				result: value,
			},
		)
	} else {
		human(&mut stdout)?;
		stdout.flush()
	}
}

pub(crate) fn failure(json: bool, error: &Failure) -> io::Result<()> {
	if json {
		write_json(
			&mut io::stdout().lock(),
			&ErrorReport {
				schema_version: SCHEMA_VERSION,
				error,
			},
		)
	} else {
		writeln!(io::stderr().lock(), "{error}")
	}
}

pub(crate) fn finish(report: io::Result<()>, status: Status, publication: Publication) -> Status {
	finish_report(report, status, publication, &mut io::stderr().lock())
}

fn finish_report(
	report: io::Result<()>,
	status: Status,
	publication: Publication,
	diagnostics: &mut dyn Write,
) -> Status {
	if report.is_ok() {
		return status;
	}

	let _ = writeln!(diagnostics, "The command result could not be delivered.");
	match (status, publication) {
		(Status::Uncertain, _) | (_, Publication::Possible) => {
			let _ = writeln!(
				diagnostics,
				"Inspect the destination and any backups or recovery files before retrying."
			);
			Status::Uncertain
		}
		_ => Status::Io,
	}
}

fn write_json(out: &mut dyn Write, value: &impl Serialize) -> io::Result<()> {
	serde_json::to_writer(&mut *out, value)?;
	writeln!(out)?;
	out.flush()
}

#[cfg(test)]
mod tests {
	use super::*;

	struct ClosedDiagnostics;

	impl Write for ClosedDiagnostics {
		fn write(&mut self, _: &[u8]) -> io::Result<usize> {
			Err(io::ErrorKind::BrokenPipe.into())
		}

		fn flush(&mut self) -> io::Result<()> {
			Err(io::ErrorKind::BrokenPipe.into())
		}
	}

	#[test]
	fn reporting_cannot_downgrade_an_uncertain_operation() {
		assert_eq!(
			finish_report(
				Err(io::ErrorKind::BrokenPipe.into()),
				Status::Uncertain,
				Publication::None,
				&mut ClosedDiagnostics,
			),
			Status::Uncertain
		);
	}

	#[test]
	fn a_delivered_report_keeps_the_original_exit_status() {
		for status in [Status::Success, Status::Conflict, Status::Uncertain] {
			let mut diagnostics = Vec::new();
			assert_eq!(
				finish_report(Ok(()), status, Publication::Possible, &mut diagnostics),
				status
			);
			assert!(diagnostics.is_empty());
		}
	}

	#[test]
	fn losing_the_location_of_a_published_backup_requires_inspection() {
		let mut diagnostics = Vec::new();
		assert_eq!(
			finish_report(
				Err(io::ErrorKind::BrokenPipe.into()),
				Status::Conflict,
				Publication::Possible,
				&mut diagnostics,
			),
			Status::Uncertain
		);
		assert!(
			String::from_utf8(diagnostics)
				.unwrap()
				.contains("before retrying")
		);
	}
}
