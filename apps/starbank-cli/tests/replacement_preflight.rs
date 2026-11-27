#![cfg(target_os = "macos")]

use std::{ffi::OsString, fs, os::unix::fs::PermissionsExt as _, path::Path};

#[path = "support/cli.rs"]
mod cli;

use cli::{Fixture, SOURCE, bank_command, command, json};

const CONTEXT: [&str; 6] = [
	"--author",
	"1-S2-1-1001",
	"--player",
	"2-S2-1-2002",
	"--bank-name",
	"Bank",
];

fn file_names(directory: &Path) -> Vec<OsString> {
	let mut entries: Vec<_> = fs::read_dir(directory)
		.expect("enumerate scratch directory")
		.map(|entry| entry.expect("directory entry").file_name())
		.collect();
	entries.sort();
	entries
}

fn make_read_only(path: &Path) {
	fs::set_permissions(path, fs::Permissions::from_mode(0o400)).expect("mark bank read-only");
}

#[test]
fn refused_in_place_signature_does_not_create_a_backup() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	make_read_only(&source);
	let names = file_names(&fixture.path);
	let args = || {
		let mut command = bank_command("sign", &source);
		command.args(CONTEXT).arg("--in-place");
		command
	};

	let preview = args().arg("--dry-run").output().expect("preview signature");
	assert_eq!(json(&preview, 0)["result"]["signature"], "valid");
	assert_eq!(file_names(&fixture.path), names);

	let rejected = args().output().expect("attempt in-place signature");
	assert_eq!(json(&rejected, 4)["error"]["code"], "io");
	assert_eq!(file_names(&fixture.path), names);
	assert_eq!(fs::read_to_string(&source).expect("read source"), SOURCE);

	let copy = fixture.path.join("Copy.sc2bank");
	let exported = bank_command("export", &source)
		.arg("--output")
		.arg(&copy)
		.args(CONTEXT)
		.output()
		.expect("export copy from read-only source");
	assert_eq!(json(&exported, 0)["result"]["signature"], "valid");
	assert!(copy.is_file());
}

#[test]
fn refused_restore_does_not_create_a_reversal_backup() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let backup = command()
		.args(["backup", "create"])
		.arg(&source)
		.arg("--json")
		.output()
		.expect("create historical snapshot");
	let backup = json(&backup, 0);
	let historic = backup["result"]["path"]["display"]
		.as_str()
		.expect("snapshot path");
	let current = SOURCE.replace("00065", "00071");
	fs::write(&source, &current).expect("update live bank");
	make_read_only(&source);
	let inspection = bank_command("inspect", &source)
		.output()
		.expect("inspect live bank");
	let inspection = json(&inspection, 0);
	let digest = inspection["result"]["sha256"]
		.as_str()
		.expect("live fingerprint");
	let names = file_names(&fixture.path);
	let restore = || {
		let mut command = command();
		command
			.args(["backup", "restore"])
			.arg(&source)
			.arg(historic)
			.args(["--expected-current-sha256", digest, "--json"]);
		command
	};

	let preview = restore()
		.arg("--dry-run")
		.output()
		.expect("preview restore");
	assert_eq!(json(&preview, 0)["result"]["outcome"], "prepared");
	assert_eq!(file_names(&fixture.path), names);

	let rejected = restore().output().expect("attempt restore");
	assert_eq!(json(&rejected, 4)["error"]["code"], "io");
	assert_eq!(file_names(&fixture.path), names);
	assert_eq!(fs::read_to_string(&source).expect("read source"), current);
	assert_eq!(fs::read_to_string(historic).expect("read snapshot"), SOURCE);
}
