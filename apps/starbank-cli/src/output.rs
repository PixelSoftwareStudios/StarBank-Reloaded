use std::io::{self, Write};

use serde::Serialize;

use crate::failure::Failure;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandName {
	Inspect,
	Validate,
	Diff,
	Discover,
	Sign,
	Edit,
	Export,
	BackupList,
	BackupCreate,
	BackupRestore,
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

fn write_json(out: &mut dyn Write, value: &impl Serialize) -> io::Result<()> {
	serde_json::to_writer(&mut *out, value)?;
	writeln!(out)?;
	out.flush()
}
