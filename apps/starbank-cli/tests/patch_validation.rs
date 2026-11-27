#![cfg(any(target_os = "macos", target_os = "linux", windows))]

use std::fs;

use serde_json::json as object;

#[path = "support/cli.rs"]
mod cli;
#[path = "support/patch.rs"]
mod patch;

use cli::{Fixture, SOURCE, bank_command, json};
use patch::{digest, edit_patch};

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
