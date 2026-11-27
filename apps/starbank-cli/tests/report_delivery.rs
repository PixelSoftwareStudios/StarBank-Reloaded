#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{fs, io, path::Path, process::Command};

use serde_json::Value;

#[path = "support/cli.rs"]
mod cli;

use cli::{Fixture, SOURCE, bank_command, command, json};

fn closed_stdout(command: &mut Command, expected_status: i32) -> String {
	let (reader, writer) = io::pipe().expect("open report pipe");
	drop(reader);
	let output = command
		.stdout(writer)
		.output()
		.expect("run CLI with closed stdout");
	assert_eq!(
		output.status.code(),
		Some(expected_status),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stderr).expect("read diagnostic")
}

fn verify_backups(bank: &Path) -> Value {
	let listed = command()
		.args(["backup", "list", "--verify", "--json"])
		.arg(bank)
		.output()
		.expect("verify recorded backups");
	json(&listed, 0)
}

#[test]
fn closed_report_distinguishes_a_preview_from_a_published_export() {
	let fixture = Fixture::new();
	let bank = fixture.write("Live.sc2bank", SOURCE);
	let copy = fixture.path.join("Copy.sc2bank");
	let export = || {
		let mut command = bank_command("export", &bank);
		command.arg("--output").arg(&copy).arg("--no-resign");
		command
	};
	let diagnostic = closed_stdout(export().arg("--dry-run"), 4);
	assert!(diagnostic.contains("command result could not be delivered"));
	assert!(!diagnostic.contains("before retrying"));
	assert!(!copy.exists());

	let diagnostic = closed_stdout(&mut export(), 6);
	assert!(diagnostic.contains("command result could not be delivered"));
	assert!(diagnostic.contains("before retrying"));
	assert_eq!(
		fs::read_to_string(copy).expect("read published copy"),
		SOURCE
	);
	assert_eq!(fs::read_to_string(bank).expect("read original"), SOURCE);
}

#[test]
fn closed_report_preserves_backup_create_effect() {
	let fixture = Fixture::new();
	let bank = fixture.write("Live.sc2bank", SOURCE);
	let mut create = command();
	create.args(["backup", "create", "--json"]).arg(&bank);
	let diagnostic = closed_stdout(&mut create, 6);
	assert!(diagnostic.contains("before retrying"));

	let listed = verify_backups(&bank);
	let backups = listed["result"]["backups"].as_array().expect("backups");
	assert_eq!(backups.len(), 1);
	let path = backups[0]["path"]["display"].as_str().expect("backup path");
	assert_eq!(backups[0]["health"], "verified");
	assert_eq!(fs::read_to_string(path).expect("read backup"), SOURCE);
	assert_eq!(fs::read_to_string(bank).expect("read original"), SOURCE);
}

#[cfg(target_os = "macos")]
#[test]
fn closed_report_preserves_in_place_write_and_reversal_effects() {
	let fixture = Fixture::new();
	let bank = fixture.write("Live.sc2bank", SOURCE);
	let context = [
		"--author",
		"1-S2-1-1001",
		"--player",
		"2-S2-1-2002",
		"--bank-name",
		"Live",
	];
	let mut sign = bank_command("sign", &bank);
	sign.args(context).arg("--in-place");
	let diagnostic = closed_stdout(&mut sign, 6);
	assert!(diagnostic.contains("before retrying"));
	let signed = fs::read(&bank).expect("read signed bank");
	assert_ne!(signed, SOURCE.as_bytes());

	let validated = bank_command("validate", &bank)
		.args(context)
		.arg("--require-signature")
		.output()
		.expect("validate published signature");
	let validated = json(&validated, 0);
	let current_digest = validated["result"]["sha256"]
		.as_str()
		.expect("signed bank digest");

	let listed = verify_backups(&bank);
	let backups = listed["result"]["backups"].as_array().expect("backups");
	assert_eq!(backups.len(), 1);
	let backup = backups[0]["path"]["display"].as_str().expect("backup path");
	assert_eq!(backups[0]["health"], "verified");
	assert_eq!(
		fs::read_to_string(backup).expect("read original backup"),
		SOURCE
	);
	let restore = || {
		let mut command = command();
		command
			.args(["backup", "restore", "--json"])
			.arg(&bank)
			.arg(backup)
			.args(["--expected-current-sha256", current_digest]);
		command
	};
	closed_stdout(restore().arg("--dry-run"), 4);
	assert_eq!(fs::read(&bank).expect("read live after preview"), signed);
	let listed = verify_backups(&bank);
	assert_eq!(
		listed["result"]["backups"]
			.as_array()
			.expect("backups")
			.len(),
		1
	);

	let diagnostic = closed_stdout(&mut restore(), 6);
	assert!(diagnostic.contains("before retrying"));
	assert_eq!(
		fs::read_to_string(&bank).expect("read restored bank"),
		SOURCE
	);
	let listed = verify_backups(&bank);
	let backups = listed["result"]["backups"].as_array().expect("backups");
	assert_eq!(backups.len(), 2);
	let reversal = backups
		.iter()
		.find(|entry| entry["path"]["display"] != backup)
		.expect("reversal backup");
	assert_eq!(reversal["health"], "verified");
	let reversal_path = reversal["path"]["display"].as_str().expect("reversal path");
	assert_eq!(
		fs::read(reversal_path).expect("read reversal bytes"),
		signed
	);
}
