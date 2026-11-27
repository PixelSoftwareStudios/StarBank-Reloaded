#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{fs, path::Path};

use serde_json::{Value, json as object};
use sha2::{Digest as _, Sha256};

#[path = "support/cli.rs"]
mod cli;

use cli::{Fixture, SOURCE, bank_command, command, json};

fn inspect(path: &Path) -> Value {
	json(
		&bank_command("inspect", path)
			.output()
			.expect("inspect input"),
		0,
	)
}

fn digest(path: &Path) -> String {
	inspect(path)["result"]["sha256"]
		.as_str()
		.expect("digest")
		.to_owned()
}

fn edit_patch(operation: Value, digest: &str, fixture: &Fixture) -> std::path::PathBuf {
	let patch = object!({
		"schema_version": 1,
		"expected_sha256": digest,
		"operations": [operation],
	});
	fixture.write(
		"patch.json",
		serde_json::to_vec(&patch).expect("serialize patch"),
	)
}

#[test]
fn edit_prepares_and_commits_the_same_exact_copy_without_clobber() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let destination = fixture.path.join("Destination.sc2bank");
	let patch = edit_patch(
		object!({"op": "set_value", "section": 0, "key": 1, "value": 0, "lexeme": "9,007,199,254,740,993"}),
		&digest(&source),
		&fixture,
	);
	let args = || {
		let mut command = bank_command("edit", &source);
		command
			.arg("--patch")
			.arg(&patch)
			.arg("--output")
			.arg(&destination);
		command
	};
	let before_count = fs::read_dir(&fixture.path)
		.expect("enumerate fixture")
		.count();
	let preview = args().arg("--dry-run").output().expect("dry-run");
	let preview = json(&preview, 0);
	assert_eq!(preview["result"]["outcome"], "prepared");
	assert_eq!(preview["result"]["revision"], "1");
	assert_eq!(preview["result"]["signature"], "unsigned");
	assert!(!destination.exists());
	assert_eq!(
		fs::read_dir(&fixture.path)
			.expect("enumerate fixture")
			.count(),
		before_count
	);

	let written = args().output().expect("edit copy");
	let written = json(&written, 0);
	assert_eq!(written["result"]["outcome"], "committed_verified");
	assert_eq!(written["result"]["diff"], preview["result"]["diff"]);
	assert_eq!(fs::read_to_string(&source).expect("read source"), SOURCE);
	assert_eq!(
		fs::read_to_string(&destination).expect("read copy"),
		SOURCE.replace("00065", "9007199254740993")
	);
	let collision = args().output().expect("repeat edit into existing file");
	assert_eq!(json(&collision, 5)["error"]["code"], "destination_exists");
	assert_eq!(
		fs::read_to_string(&destination).expect("read existing copy"),
		SOURCE.replace("00065", "9007199254740993")
	);
}

#[test]
fn patches_require_the_current_digest_and_known_fields_and_fail_atomically() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let destination = fixture.path.join("Destination.sc2bank");
	let bad_operation = object!({
		"op":"set_value","section":0,"key":1,"value":0,"lexeme":"7",
	});
	let patch = edit_patch(bad_operation.clone(), &"0".repeat(64), &fixture);
	let args = || {
		let mut command = bank_command("edit", &source);
		command
			.arg("--patch")
			.arg(&patch)
			.arg("--output")
			.arg(&destination);
		command
	};
	let stale = args().output().expect("run stale patch");
	assert_eq!(json(&stale, 5)["error"]["code"], "input_changed");

	let current_digest = digest(&source);
	let unsupported = r#"{"schema_version":2,"expected_sha256":"CURRENT_DIGEST","operations":[{"op":"future_operation"}],"future_field":true}"#;
	let malformed = [
		r#"{"schema_version":1,"expected_sha256":"CURRENT_DIGEST","operations":[{"op":"future_operation"}]}"#,
		r#"{"schema_version":2,"operations":[}"#,
		r#"{"operations":[]}"#,
		r#"{"schema_version":2.5}"#,
		r#"{"schema_version":2,"schema_version":1}"#,
		"[2]",
	]
	.map(|body| (body, 3, "invalid_patch"));
	for (body, status, code) in [(unsupported, 7, "unsupported_patch_version")]
		.into_iter()
		.chain(malformed)
	{
		fixture.write(
			"patch.json",
			body.replace("CURRENT_DIGEST", &current_digest),
		);
		for flags in [&[][..], &["--dry-run"][..]] {
			let output = args().args(flags).output().expect("run versioned patch");
			assert_eq!(json(&output, status)["error"]["code"], code);
		}
	}

	let mut unknown_field = bad_operation.clone();
	unknown_field["section_name"] = object!("must be rejected");
	edit_patch(unknown_field, &current_digest, &fixture);
	let unknown_field = args().output().expect("run patch with unknown field");
	assert_eq!(json(&unknown_field, 3)["error"]["code"], "invalid_patch");

	let invalid_batch = object!({
		"schema_version": 1,
		"expected_sha256": current_digest,
		"operations": [
			bad_operation,
			{"op":"set_value","section":0,"key":0,"value":0,"lexeme":"12,34"}
		]
	});
	fixture.write("patch.json", invalid_batch.to_string());
	let invalid_batch = args().output().expect("run invalid batch");
	assert_eq!(json(&invalid_batch, 3)["error"]["code"], "invalid_edit");
	let positional = object!([1, current_digest, [bad_operation]]);
	fixture.write("patch.json", positional.to_string());
	let preview = args()
		.arg("--dry-run")
		.output()
		.expect("preview positional patch");
	assert_eq!(json(&preview, 0)["result"]["outcome"], "prepared");
	assert!(!destination.exists());
	let entries = fs::read_dir(&fixture.path).expect("read fixture directory");
	assert_eq!(entries.count(), 2);
	assert_eq!(fs::read_to_string(&source).expect("read source"), SOURCE);
}

#[test]
fn edit_checks_the_observed_fingerprint_before_parsing_replaced_bank_bytes() {
	const MALFORMED_BANK: &[u8] = b"<Bank><Section";
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let destination = fixture.path.join("Destination.sc2bank");
	let operation = object!({
		"op": "set_value", "section": 0, "key": 1, "value": 0, "lexeme": "7",
	});
	let patch = edit_patch(operation.clone(), &digest(&source), &fixture);
	fs::write(&source, MALFORMED_BANK).expect("replace observed source");
	let args = || {
		let mut command = bank_command("edit", &source);
		command
			.arg("--patch")
			.arg(&patch)
			.arg("--output")
			.arg(&destination);
		command
	};

	let preview = args()
		.arg("--dry-run")
		.output()
		.expect("preview stale edit");
	assert_eq!(json(&preview, 5)["error"]["code"], "input_changed");
	let stale = args().output().expect("apply stale edit");
	assert_eq!(json(&stale, 5)["error"]["code"], "input_changed");

	let current_sha256 = hex::encode(Sha256::digest(MALFORMED_BANK));
	edit_patch(operation, &current_sha256, &fixture);
	let matching = args().output().expect("apply matching edit");
	assert_eq!(json(&matching, 3)["error"]["code"], "invalid_bank");
	assert!(!destination.exists());
	assert_eq!(
		fs::read(&source).expect("read replaced source"),
		MALFORMED_BANK
	);
}

const CONTEXT: [&str; 6] = [
	"--author",
	"1-S2-1-1001",
	"--player",
	"2-S2-1-2002",
	"--bank-name",
	"Bank",
];

#[test]
fn sign_requires_an_explicit_target_for_effect_and_verifies_lossless_output() {
	let fixture = Fixture::new();
	let original =
		include_str!("../../../crates/domain/starbank-signature/tests/fixtures/case-ties.sc2bank");
	let source = fixture.write("Source.sc2bank", original);
	let destination = fixture.path.join("Signed.sc2bank");

	let no_target = bank_command("sign", &source)
		.args(CONTEXT)
		.output()
		.expect("missing target");
	assert_eq!(json(&no_target, 2)["error"]["code"], "invalid_arguments");
	let preview = bank_command("sign", &source)
		.args(CONTEXT)
		.arg("--dry-run")
		.output()
		.expect("preview signature");
	let preview = json(&preview, 0);
	assert_eq!(preview["result"]["signature"], "valid");
	assert_eq!(
		preview["result"]["diff"]["stored_signature"]["after"],
		"400CCD1D17B653D68BE2FE645A694FB016C3105D"
	);
	assert_eq!(
		fs::read_dir(&fixture.path)
			.expect("enumerate fixture")
			.count(),
		1
	);

	let output = bank_command("sign", &source)
		.args(CONTEXT)
		.arg("--output")
		.arg(&destination)
		.output()
		.expect("write signature to copy");
	let output = json(&output, 0);
	assert_eq!(output["result"]["diff"], preview["result"]["diff"]);
	assert_eq!(
		fs::read_to_string(&source).expect("read original"),
		original
	);
	let verified = bank_command("validate", &destination)
		.args(CONTEXT)
		.arg("--require-signature")
		.output()
		.expect("verify persisted signature");
	assert_eq!(json(&verified, 0)["result"]["signature"]["status"], "valid");

	let unconfirmed = bank_command("export", &destination)
		.arg("--output")
		.arg(fixture.path.join("Rejected.sc2bank"))
		.output()
		.expect("export without signing context");
	assert_eq!(json(&unconfirmed, 3)["error"]["code"], "stale_signature");
	assert!(!fixture.path.join("Rejected.sc2bank").exists());
}

#[test]
fn backup_catalog_survives_reopening_and_validates_snapshot_integrity() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let create = || {
		command()
			.args(["backup", "create"])
			.arg(&source)
			.arg("--json")
			.output()
			.expect("create verified backup")
	};
	let first = json(&create(), 0);
	let second = json(&create(), 0);
	let first_path = first["result"]["path"]["display"]
		.as_str()
		.expect("snapshot path");
	let second_path = second["result"]["path"]["display"]
		.as_str()
		.expect("snapshot path");
	assert_ne!(
		first_path, second_path,
		"a second backup must never clobber the first"
	);
	assert_eq!(fs::read_to_string(first_path).expect("read backup"), SOURCE);
	assert_eq!(
		fs::read_to_string(second_path).expect("read backup"),
		SOURCE
	);

	let list = || {
		command()
			.args(["backup", "list"])
			.arg(&source)
			.args(["--verify", "--json"])
			.output()
			.expect("list durable backups")
	};
	let listed = json(&list(), 0);
	let backups = listed["result"]["backups"]
		.as_array()
		.expect("list of backups");
	assert_eq!(backups.len(), 2);
	assert!(backups.iter().all(|entry| entry["health"] == "verified"));

	fs::write(first_path, b"modified snapshot").expect("damage synthetic snapshot");
	let listed = json(&list(), 0);
	assert!(
		listed["result"]["backups"]
			.as_array()
			.expect("list of backups")
			.iter()
			.any(|entry| entry["path"]["display"] == first_path && entry["health"] == "unverified")
	);
}

#[test]
fn backup_restore_previews_exact_bytes_and_preserves_reversal_on_commit() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let create = command()
		.args(["backup", "create"])
		.arg(&source)
		.arg("--json")
		.output()
		.expect("create historical backup");
	let create = json(&create, 0);
	let historic = create["result"]["path"]["display"]
		.as_str()
		.expect("backup path");
	let current = SOURCE.replace("00065", "00071");
	fs::write(&source, &current).expect("modify synthetic bank");
	let known_digest = digest(&source);
	let restore = |digest: &str| {
		let mut command = command();
		command
			.args(["backup", "restore"])
			.arg(&source)
			.arg(historic)
			.args(["--expected-current-sha256", digest, "--json"]);
		command
	};
	let failed = restore(&"0".repeat(64))
		.output()
		.expect("restore stale target");
	assert_eq!(json(&failed, 5)["error"]["code"], "input_changed");

	let before_count = fs::read_dir(&fixture.path)
		.expect("enumerate fixture")
		.count();
	let dry_run = restore(&known_digest)
		.arg("--dry-run")
		.output()
		.expect("preview restore");
	let dry_run = json(&dry_run, 0);
	assert_eq!(dry_run["result"]["outcome"], "prepared");
	assert_eq!(dry_run["result"]["before_sha256"], known_digest);
	assert_eq!(
		dry_run["result"]["after_sha256"],
		create["result"]["sha256"]
	);
	assert_eq!(fs::read_to_string(&source).expect("read live"), current);
	assert_eq!(
		fs::read_dir(&fixture.path)
			.expect("enumerate fixture")
			.count(),
		before_count
	);

	let commit = restore(&known_digest)
		.output()
		.expect("restore verified bytes");
	#[cfg(target_os = "macos")]
	{
		let result = &json(&commit, 0)["result"];
		assert_eq!(result["outcome"], "committed_verified");
		let reversal = result["reversal_backup"].as_str().expect("reversal backup");
		assert_eq!(
			fs::read_to_string(reversal).expect("read reversal"),
			current
		);
		assert_eq!(fs::read_to_string(&source).expect("read live"), SOURCE);
	}
	#[cfg(target_os = "linux")]
	{
		assert_eq!(json(&commit, 7)["error"]["code"], "unsupported_storage");
		assert_eq!(fs::read_to_string(&source).expect("read live"), current);
		assert_eq!(
			fs::read_dir(&fixture.path)
				.expect("enumerate fixture")
				.count(),
			before_count
		);
	}
}

#[test]
fn structural_patch_targets_original_occurrences_after_projected_order_changes() {
	let fixture = Fixture::new();
	let source = fixture.write("Source.sc2bank", SOURCE);
	let destination = fixture.path.join("Destination.sc2bank");
	let patch = fixture.write(
		"patch.json",
		serde_json::to_vec(&object!({
			"schema_version": 1,
			"expected_sha256": digest(&source),
			"operations": [
				{"op":"remove_key","section":0,"key":0},
				{"op":"rename_key","section":0,"key":1,"name":"Renamed"},
				{"op":"set_value","section":0,"key":1,"value":0,"lexeme":"1,000"},
				{"op":"add_value","section":0,"key":1,"name":"Score","kind":"fixed","lexeme":"1,234.50"}
			]
		}))
		.expect("serialize patch containing structure and values"),
	);
	let output = bank_command("edit", &source)
		.arg("--patch")
		.arg(patch)
		.arg("--output")
		.arg(&destination)
		.output()
		.expect("apply structural patch");
	assert_eq!(json(&output, 0)["result"]["outcome"], "committed_verified");
	let inspected = inspect(&destination);
	let keys = inspected["result"]["contents"][0]["keys"]
		.as_array()
		.expect("projected keys");
	assert_eq!(keys.len(), 1);
	assert_eq!(keys[0]["name"], "Renamed");
	assert_eq!(keys[0]["values"][0]["value"], "1000");
	assert_eq!(keys[0]["values"][2]["name"], "Score");
	assert_eq!(keys[0]["values"][2]["value"], "1234.50");
	assert_eq!(fs::read_to_string(&source).expect("read original"), SOURCE);
}

#[test]
fn utf16_exports_and_edits_keep_the_original_byte_order() {
	let text =
		format!("<?xml version='1.0' encoding='UTF-16'?>\r\n{SOURCE}").replace("Player", "星😀");
	for little in [true, false] {
		let encode = |xml: &str| -> Vec<u8> {
			let units = std::iter::once(0xfeff).chain(xml.encode_utf16());
			if little {
				units.flat_map(u16::to_le_bytes).collect()
			} else {
				units.flat_map(u16::to_be_bytes).collect()
			}
		};
		let fixture = Fixture::new();
		let bytes = encode(&text);
		let source = fixture.write("Source.sc2bank", &bytes);
		let unchanged = fixture.path.join("Unchanged.sc2bank");
		let output = bank_command("export", &source)
			.arg("--no-resign")
			.arg("--output")
			.arg(&unchanged)
			.output()
			.expect("export UTF-16 bank");
		assert_eq!(json(&output, 0)["result"]["outcome"], "committed_verified");
		assert_eq!(fs::read(&unchanged).expect("read unedited export"), bytes);

		let patch = edit_patch(
			object!({"op":"set_value","section":0,"key":1,"value":0,"lexeme":"1,001"}),
			&digest(&source),
			&fixture,
		);
		let destination = fixture.path.join("Edited.sc2bank");
		let output = bank_command("edit", &source)
			.arg("--patch")
			.arg(patch)
			.arg("--no-resign")
			.arg("--output")
			.arg(&destination)
			.output()
			.expect("edit UTF-16 bank");
		assert_eq!(json(&output, 0)["result"]["outcome"], "committed_verified");
		assert_eq!(fs::read(&source).expect("read source"), bytes);
		assert_eq!(
			fs::read(&destination).expect("read edited export"),
			encode(&text.replace("00065", "1001"))
		);
	}
}
