#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{
	collections::BTreeMap,
	ffi::OsString,
	fs,
	path::{Path, PathBuf},
	process::Command,
};

use sha2::{Digest as _, Sha256};

#[path = "support/cli.rs"]
mod cli;

use cli::{Fixture, SOURCE, bank_command, command, json};

const CANONICAL_BANK: &str = "Accounts/123/2-S2-1-2002/Banks/1-S2-1-1001/Parity.sc2bank";

fn backup(bank: &Path) -> PathBuf {
	let result = command()
		.args(["backup", "create"])
		.arg(bank)
		.arg("--json")
		.output()
		.expect("create backup");
	let report = json(&result, 0);
	PathBuf::from(
		report["result"]["path"]["display"]
			.as_str()
			.expect("backup path"),
	)
}

fn restore(bank: &Path, selected: &Path, expected_sha256: &str) -> Command {
	let mut command = command();
	command
		.args(["backup", "restore"])
		.arg(bank)
		.arg(selected)
		.args(["--expected-current-sha256", expected_sha256, "--json"]);
	command
}

fn directory_contents(bank: &Path) -> BTreeMap<OsString, Vec<u8>> {
	fs::read_dir(bank.parent().expect("live bank has a parent"))
		.expect("read fixture directory")
		.map(|entry| {
			let entry = entry.expect("directory entry");
			let bytes = fs::read(entry.path()).expect("read fixture file");
			(entry.file_name(), bytes)
		})
		.collect()
}

#[test]
fn restore_preview_reports_the_diff_and_destination_signature() {
	let fixture = Fixture::new();
	let unsigned = fixture.write("Unsigned.sc2bank", SOURCE);
	let bank = fixture.path.join(CANONICAL_BANK);
	fs::create_dir_all(bank.parent().expect("live bank has a parent")).expect("create bank path");
	let signed = bank_command("sign", &unsigned)
		.arg("--output")
		.arg(&bank)
		.output()
		.expect("sign into intended destination");
	let signed = json(&signed, 0);
	let signature = &signed["result"]["diff"]["stored_signature"]["after"];
	let backup = backup(&bank);
	let stored = fs::read_to_string(&bank).expect("read signed bank");
	let current = stored.replace("00065", "00071");
	fs::write(&bank, &current).expect("change bank while preserving its previous signature");
	let contents_before = directory_contents(&bank);

	let expected_sha256 = hex::encode(Sha256::digest(&current));
	let preview = restore(&bank, &backup, &expected_sha256)
		.arg("--dry-run")
		.output()
		.expect("preview restore");
	let preview = &json(&preview, 0)["result"];
	let standalone = bank_command("diff", &bank)
		.arg(&backup)
		.output()
		.expect("compare live bank with backup");
	let diff = &json(&standalone, 0)["result"];
	assert_eq!(preview["diff"], *diff);
	assert_eq!(preview["before_sha256"], diff["before_sha256"]);
	assert_eq!(preview["after_sha256"], diff["after_sha256"]);
	assert_eq!(preview["signature"]["status"], "valid");
	assert_eq!(preview["signature"]["digest"], *signature);
	assert!(preview["comparison_diagnostic"].is_null());
	assert_eq!(preview["outcome"], "prepared");
	assert_eq!(directory_contents(&bank), contents_before);
}

#[test]
fn restore_reports_unavailable_comparison_and_preserves_invalid_signature_bytes() {
	const MALFORMED_LIVE: &[u8] = b"<Bank><Section";
	const INVALID_SIGNATURE: &str = "0000000000000000000000000000000000000000";
	let fixture = Fixture::new();
	let stored = SOURCE.replace(
		"</Bank>",
		&format!("<Signature value='{INVALID_SIGNATURE}' /></Bank>"),
	);
	let bank = fixture.write(CANONICAL_BANK, &stored);
	let backup = backup(&bank);
	fs::write(&bank, MALFORMED_LIVE).expect("simulate malformed live bank");
	let contents_before = directory_contents(&bank);
	let expected_sha256 = hex::encode(Sha256::digest(MALFORMED_LIVE));

	let preview = restore(&bank, &backup, &expected_sha256)
		.arg("--dry-run")
		.output()
		.expect("preview recovery");
	let preview = &json(&preview, 0)["result"];
	assert_eq!(preview["outcome"], "prepared");
	assert_eq!(preview["before_sha256"], expected_sha256);
	assert!(preview["diff"].is_null());
	assert_eq!(preview["comparison_diagnostic"]["code"], "invalid_bank");
	assert_eq!(preview["signature"]["status"], "invalid");
	assert_eq!(preview["signature"]["digest"], INVALID_SIGNATURE);
	assert_eq!(directory_contents(&bank), contents_before);

	let committed = restore(&bank, &backup, &expected_sha256)
		.output()
		.expect("recover malformed live bank");
	#[cfg(target_os = "macos")]
	{
		let report = &json(&committed, 0)["result"];
		assert_eq!(report["outcome"], "committed_verified");
		assert_eq!(report["signature"], preview["signature"]);
		assert_eq!(report["after_sha256"], preview["after_sha256"]);
		assert_eq!(
			report["comparison_diagnostic"],
			preview["comparison_diagnostic"]
		);
		let reversal = report["reversal_backup"].as_str().expect("reversal path");
		assert_eq!(fs::read(reversal).expect("read reversal"), MALFORMED_LIVE);
		assert_eq!(
			fs::read(&bank).expect("read restored bank"),
			stored.as_bytes()
		);
		assert_eq!(
			fs::read(&backup).expect("read verified backup"),
			stored.as_bytes()
		);
	}
	#[cfg(target_os = "linux")]
	{
		assert_eq!(json(&committed, 7)["error"]["code"], "unsupported_storage");
		assert_eq!(directory_contents(&bank), contents_before);
	}
}
