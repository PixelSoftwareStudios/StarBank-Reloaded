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

#[test]
fn restore_refuses_a_replaced_backup_when_the_reviewed_digest_is_supplied() {
	let fixture = Fixture::new();
	let bank = fixture.write("Live.sc2bank", SOURCE);
	let selected = backup(&bank);
	let current = SOURCE.replace("00065", "00071");
	fs::write(&bank, &current).expect("modify live bank");
	let expected_current = hex::encode(Sha256::digest(&current));
	let initial = restore(&bank, &selected, &expected_current)
		.arg("--dry-run")
		.output()
		.expect("review original backup");
	let initial = json(&initial, 0);
	let expected_backup = initial["result"]["after_sha256"]
		.as_str()
		.expect("reviewed backup digest");

	let replacement = SOURCE.replace("00065", "00099");
	let replacement_digest = hex::encode(Sha256::digest(&replacement));
	fs::write(&selected, &replacement).expect("replace synthetic backup bytes");
	let mut sidecar = selected.as_os_str().to_owned();
	sidecar.push(".json");
	let mut metadata: serde_json::Value =
		serde_json::from_slice(&fs::read(&sidecar).expect("read backup sidecar"))
			.expect("decode backup sidecar");
	metadata["sha256"] = serde_json::json!(replacement_digest);
	fs::write(
		&sidecar,
		serde_json::to_vec(&metadata).expect("encode replacement sidecar"),
	)
	.expect("update synthetic backup sidecar");
	let contents_before = directory_contents(&bank);

	let unpinned = restore(&bank, &selected, &expected_current)
		.arg("--dry-run")
		.output()
		.expect("review the internally consistent replacement");
	assert_eq!(
		json(&unpinned, 0)["result"]["after_sha256"],
		replacement_digest
	);
	let guarded = |digest: &str| {
		let mut command = restore(&bank, &selected, &expected_current);
		command.args(["--expected-backup-sha256", digest]);
		command
	};
	let malformed = guarded(&"z".repeat(64))
		.arg("--dry-run")
		.output()
		.expect("reject malformed digest");
	assert_eq!(json(&malformed, 3)["error"]["code"], "invalid_backup");

	let stale_preview = guarded(expected_backup)
		.arg("--dry-run")
		.output()
		.expect("reject stale digest during preview");
	assert_eq!(json(&stale_preview, 5)["error"]["code"], "backup_changed");
	let stale_apply = guarded(expected_backup)
		.output()
		.expect("reject stale digest before applying");
	assert_eq!(json(&stale_apply, 5)["error"]["code"], "backup_changed");

	let matching = guarded(&replacement_digest.to_ascii_uppercase())
		.arg("--dry-run")
		.output()
		.expect("preview the newly reviewed backup with its digest");
	assert_eq!(
		json(&matching, 0)["result"]["after_sha256"],
		replacement_digest
	);
	assert_eq!(directory_contents(&bank), contents_before);

	let applied = guarded(&replacement_digest)
		.output()
		.expect("apply the newly reviewed backup with its digest");
	#[cfg(target_os = "macos")]
	{
		let report = json(&applied, 0);
		assert_eq!(report["result"]["outcome"], "committed_verified");
		assert_eq!(report["result"]["after_sha256"], replacement_digest);
		let reversal = report["result"]["reversal_backup"]
			.as_str()
			.expect("reversal snapshot");
		assert_eq!(
			fs::read_to_string(reversal).expect("read reversal"),
			current
		);
		assert_eq!(
			fs::read_to_string(&bank).expect("read restored bank"),
			replacement
		);
		assert_eq!(
			fs::read_to_string(&selected).expect("read selected backup"),
			replacement
		);
	}
	#[cfg(target_os = "linux")]
	{
		assert_eq!(json(&applied, 7)["error"]["code"], "unsupported_storage");
		assert_eq!(directory_contents(&bank), contents_before);
	}
}
