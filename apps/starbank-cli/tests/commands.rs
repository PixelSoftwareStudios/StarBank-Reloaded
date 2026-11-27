use std::{ffi::OsStr, fs, process::Command};

#[path = "support/cli.rs"]
mod cli;

use cli::{Fixture, SOURCE, bank_command, command, json};

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn inspect_preserves_exact_values_and_duplicate_occurrences() {
	let fixture = Fixture::new();
	let path = fixture.write("Player.sc2bank", SOURCE);
	let output = bank_command("inspect", &path)
		.output()
		.expect("run inspect");
	let body = json(&output, 0);
	assert_eq!(body["command"], "inspect");
	let result = &body["result"];
	assert_eq!(result["sections"], 1);
	assert_eq!(result["keys"], 2);
	assert_eq!(result["signature"]["status"], "unsigned");
	let keys = &result["contents"][0]["keys"];
	assert_eq!(keys[0]["name"], "Points");
	assert_eq!(keys[1]["name"], "Points");
	assert_eq!(keys[0]["values"][0]["value"], "9007199254740993.050");
	assert_eq!(keys[1]["values"][0]["value"], "00065");
	assert_eq!(keys[1]["values"][1]["value"], "A & B");
	assert_eq!(result["sha256"].as_str().expect("SHA-256 string").len(), 64);
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn validate_reports_typed_status_and_anonymized_diagnostics() {
	let fixture = Fixture::new();
	let path = fixture.write(
		"private-label.sc2bank",
		b"<Bank>\n<Section name=\"private content\"></Bank>",
	);
	let output = bank_command("validate", &path)
		.output()
		.expect("run validate for malformed XML");
	let body = json(&output, 3);
	assert_eq!(body["error"]["code"], "invalid_bank");
	assert_eq!(body["error"]["location"]["line"], 2);
	let stdout = String::from_utf8(output.stdout).expect("JSON is UTF-8");
	assert!(!stdout.contains("private"));

	let path = fixture.write("Unsigned.sc2bank", SOURCE);
	let output = bank_command("validate", &path)
		.arg("--require-signature")
		.output()
		.expect("run validate for unsigned XML");
	assert_eq!(
		json(&output, 3)["result"]["signature"]["status"],
		"unsigned"
	);
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn signed_fixture_and_write_previews_match_across_native_encodings() {
	use sc2bank::XmlEncoding;

	const BANK_XML: &str = include_str!(
		"../../../crates/domain/starbank-signature/tests/fixtures/canonical-ascii.sc2bank"
	);
	const CONTEXT: [&str; 6] = [
		"--author",
		"1-S2-1-0",
		"--player",
		"1-S2-1-0",
		"--bank-name",
		"Fixture",
	];
	const SIGNATURE: &str = "9689A338F3C1C57C9090E476B6A5F60336C17DAC";
	// Golden SHA-256 values were computed from the independently encoded fixture bytes.
	for (encoding, declaration, unsigned_sha256, signed_sha256) in [
		(
			XmlEncoding::Utf8,
			"UTF-8",
			"7ba65c405e67fae253fadd3a9fe8e39f83527344e4b1e76c4a2ece12de799c4c",
			"efe10915dacf3a6be91325c4d0587e0d57c7e1a765787cff971c91b9f74992b1",
		),
		(
			XmlEncoding::Utf16Le,
			"UTF-16",
			"c9d6a438008c7f855abb64ada0d64481ec5a884da2632372c2ad5eb5205e1b24",
			"9cc1778924a2f470c024765faf31cc289f100c60425262e4c1b9f2fc174038ae",
		),
		(
			XmlEncoding::Utf16Be,
			"UTF-16BE",
			"8e0f3374ede8376940f320e09d867eb9c8472473090cea84694b797307c74dab",
			"df70d4cb12f26dc615ab2d14ced1dfee25066a6bd61ac221a6ab1e5e33b75bad",
		),
	] {
		let fixture = Fixture::new();
		let unsigned_xml =
			format!("<?xml version=\"1.0\" encoding=\"{declaration}\"?>\r\n{BANK_XML}");
		let signed_xml = unsigned_xml.replace(
			"</Bank>",
			&format!("<Signature value=\"{SIGNATURE}\"/></Bank>"),
		);
		let unsigned = encode_cli_fixture(encoding, &unsigned_xml);
		let signed = encode_cli_fixture(encoding, &signed_xml);
		let unsigned_path = fixture.write("Unsigned.sc2bank", &unsigned);
		let signed_path = fixture.write("Outside.sc2bank", &signed);
		let sign_target = fixture.path.join("SignDestination.sc2bank");
		let export_target = fixture.path.join("ExportDestination.sc2bank");

		let output = bank_command("inspect", &signed_path)
			.output()
			.expect("inspect signed fixture");
		let inspection = json(&output, 0);
		assert_eq!(inspection["command"], "inspect");
		let result = &inspection["result"];
		assert_eq!(result["sha256"], signed_sha256);
		assert_eq!(result["signature"]["status"], "missing_context");
		let keys = &result["contents"][1]["keys"];
		assert_eq!(keys[1]["name"], "Same");
		assert_eq!(keys[2]["name"], "Same");
		assert_eq!(keys[1]["values"][0]["value"], "A & B");
		assert_eq!(keys[1]["values"][1]["value"], "1.50");
		assert_eq!(keys[2]["values"][0]["value"], "9007199254740993");

		let output = bank_command("validate", &signed_path)
			.output()
			.expect("validate without explicit context");
		assert_eq!(
			json(&output, 0)["result"]["signature"]["status"],
			"missing_context"
		);

		let output = bank_command("validate", &signed_path)
			.args(CONTEXT)
			.arg("--require-signature")
			.output()
			.expect("validate with exact signing context");
		let validation = json(&output, 0);
		assert_eq!(validation["command"], "validate");
		assert_eq!(validation["result"]["signature"]["status"], "valid");
		assert_eq!(validation["result"]["signature"]["digest"], SIGNATURE);

		let sign = bank_command("sign", &unsigned_path)
			.args(CONTEXT)
			.args(["--dry-run", "--output"])
			.arg(&sign_target)
			.output()
			.expect("preview signature");
		let export = bank_command("export", &signed_path)
			.args(CONTEXT)
			.args(["--dry-run", "--no-resign", "--output"])
			.arg(&export_target)
			.output()
			.expect("preview export");
		for (command, output, before_sha256) in [
			("sign", sign, unsigned_sha256),
			("export", export, signed_sha256),
		] {
			let preview = json(&output, 0);
			assert_eq!(preview["command"], command);
			let result = &preview["result"];
			assert_eq!(result["outcome"], "prepared");
			assert_eq!(result["signature"], "valid");
			assert_eq!(result["diff"]["before_sha256"], before_sha256);
			assert_eq!(result["diff"]["after_sha256"], signed_sha256);
			assert_eq!(result["diff"]["stored_signature"]["after"], SIGNATURE);
			assert_eq!(
				result["diff"]["source_changed"],
				before_sha256 != signed_sha256
			);
			assert!(result["backup"].is_null());
			assert!(result["recovery"].is_null());
		}

		assert!(!sign_target.exists());
		assert!(!export_target.exists());
		assert_eq!(
			fs::read_dir(&fixture.path)
				.expect("read fixture directory")
				.count(),
			2
		);
		assert_eq!(
			fs::read(&signed_path).expect("read original signed bank"),
			signed
		);
		assert_eq!(
			fs::read(&unsigned_path).expect("read original unsigned bank"),
			unsigned
		);
	}
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn encode_cli_fixture(encoding: sc2bank::XmlEncoding, xml: &str) -> Vec<u8> {
	use sc2bank::XmlEncoding;

	const UTF16_BOM: u16 = 0xfeff;
	let units = std::iter::once(UTF16_BOM).chain(xml.encode_utf16());
	match encoding {
		XmlEncoding::Utf8 => xml.as_bytes().to_vec(),
		XmlEncoding::Utf16Le => units.flat_map(u16::to_le_bytes).collect(),
		XmlEncoding::Utf16Be => units.flat_map(u16::to_be_bytes).collect(),
	}
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn diff_disambiguates_by_order_and_reports_opaque_source_only_changes() {
	let fixture = Fixture::new();
	let before = fixture.write("Before.sc2bank", SOURCE);
	let after = fixture.write("After.sc2bank", SOURCE.replace("00065", "00066"));
	let output = command()
		.arg("diff")
		.arg(&before)
		.arg(after)
		.arg("--json")
		.output()
		.expect("run diff");
	let result = &json(&output, 0)["result"];
	assert_eq!(result["source_changed"], true);
	let key = &result["sections"][0]["keys"][0];
	assert_eq!(key["index"], 1);
	assert_eq!(key["values"][0]["before"]["value"], "00065");
	assert_eq!(key["values"][0]["after"]["value"], "00066");

	let after = fixture.write(
		"Comment.sc2bank",
		format!("<!-- unchanged fields -->\n{SOURCE}"),
	);
	let output = command()
		.arg("diff")
		.arg(before)
		.arg(after)
		.arg("--json")
		.output()
		.expect("run diff for comment-only change");
	let result = &json(&output, 0)["result"];
	assert_eq!(result["source_changed"], true);
	assert_eq!(result["sections"], serde_json::json!([]));
}

#[test]
fn discover_reports_live_and_backup_files_without_reading_bank_contents() {
	let fixture = Fixture::new();
	fixture.write("Banks/First.sc2bank", b"not XML");
	fixture.write("Banks/First_20261126_100000.sc2bank.bak", b"older bank");
	fixture.write("Loose.sc2bank", b"not XML either");
	let output = command()
		.args(["discover", "--root"])
		.arg(&fixture.path)
		.arg("--json")
		.output()
		.expect("run discovery");
	let result = &json(&output, 0)["result"];
	assert_eq!(result["banks"].as_array().expect("live banks").len(), 2);
	assert_eq!(result["backups"].as_array().expect("backup files").len(), 1);
	assert_eq!(result["root"]["lossless"], true);
}

#[test]
fn json_argument_errors_do_not_leak_arguments() {
	let output = command()
		.args([
			OsStr::new("inspect"),
			OsStr::new("--private-argument"),
			OsStr::new("--json"),
		])
		.output()
		.expect("run invalid command");
	let body = json(&output, 2);
	assert_eq!(body["error"]["code"], "invalid_arguments");
	assert!(!String::from_utf8_lossy(&output.stdout).contains("private-argument"));
}

#[test]
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn unavailable_storage_has_an_explicit_exit_code() {
	let fixture = Fixture::new();
	let path = fixture.write("Bank.sc2bank", SOURCE);
	let output = bank_command("inspect", &path)
		.output()
		.expect("run inspect");
	assert_eq!(json(&output, 7)["error"]["code"], "unsupported_storage");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn inspection_matches_portable_engine_values_and_signature_status() {
	let fixture = Fixture::new();
	let path = fixture.write("Projection.sc2bank", SOURCE);
	let output = bank_command("inspect", &path)
		.output()
		.expect("inspect source");
	let inspection = json(&output, 0);
	let mut engine = starbank_editor::Engine::default();
	let opened = engine.dispatch(
		r#"{"schema_version":1,"request_id":"test","command":{"kind":"open"}}"#,
		Some(SOURCE.as_bytes()),
	);
	let wire: serde_json::Value =
		serde_json::from_str(&opened.json).expect("decode engine projection");
	assert_eq!(wire["result"]["kind"], "snapshot");
	let snapshot = &wire["result"]["payload"];
	let result = &inspection["result"];
	assert_eq!(result["signature"]["status"], snapshot["signature_status"]);
	let cli = result["contents"][0]["keys"].as_array().expect("CLI keys");
	let engine = snapshot["sections"][0]["keys"]
		.as_array()
		.expect("engine keys");
	assert_eq!(cli.len(), engine.len());
	for (cli, engine) in cli.iter().zip(engine) {
		assert_eq!(cli["name"], engine["name"]);
		let cli_values = cli["values"].as_array().expect("CLI values");
		let engine_values = engine["values"].as_array().expect("engine values");
		assert_eq!(cli_values.len(), engine_values.len());
		for (cli, engine) in cli_values.iter().zip(engine_values) {
			assert_eq!(cli["name"], engine["name"]);
			assert_eq!(cli["kind"], engine["kind"]);
			assert_eq!(cli["value"], engine["value"]);
		}
	}
}

const ALPHA: &[u8] =
	include_bytes!("../../../crates/discovery/starbank-map-index/tests/fixtures/alpha.s2ma");
const RIVAL: &[u8] =
	include_bytes!("../../../crates/discovery/starbank-map-index/tests/fixtures/rival.s2ma");
const UNSUPPORTED_ARCHIVE: &[u8] =
	include_bytes!("../../../crates/adapters/starbank-mpq/tests/fixtures/v1-list.s2ma");

fn map(command_name: &str) -> Command {
	let mut process = command();
	process.args(["map", command_name, "--json"]);
	process
}

#[test]
fn inspect_returns_versioned_metadata_without_modifying_the_selected_archive() {
	let fixture = Fixture::new();
	let file = fixture.write("Fixture.sC2mAp", ALPHA);
	let output = map("inspect")
		.arg(&file)
		.output()
		.expect("inspect synthetic map");
	let result = json(&output, 0);
	assert_eq!(result["command"], "map_inspect");
	let report = &result["result"];
	assert_eq!(report["title"], "Fixture Alpha");
	assert_eq!(report["title_source"], "script");
	assert_eq!(report["author"], "Blizzard Entertainment");
	assert_eq!(
		report["bank_names"],
		serde_json::json!(["second", "shared"])
	);
	assert_eq!(report["has_script"], true);
	assert_eq!(report["declared_blizzard"], true);
	assert!(report["modified_unix_nanos"].as_str().is_some());
	assert_eq!(
		report["path"]["display"].as_str(),
		file.to_str(),
		"report preserves the selected local path"
	);
	assert_eq!(fs::read(&file).expect("read unchanged archive"), ALPHA);
	assert_eq!(
		fs::read_dir(&fixture.path)
			.expect("inspect fixture directory")
			.count(),
		1
	);
}

#[test]
fn inspect_rejects_directories_other_formats_and_unsupported_archives() {
	let fixture = Fixture::new();
	let directory = fixture.path.join("Directory.SC2Map");
	fs::create_dir(&directory).expect("create map-named directory");
	let mut cases = vec![(directory, 3, "invalid_map_selection")];
	cases.push((
		fixture.write("WrongExtension.txt", ALPHA),
		3,
		"invalid_map_selection",
	));
	cases.push((
		fixture.write("Unsupported.SC2Map", UNSUPPORTED_ARCHIVE),
		7,
		"unsupported_map_archive",
	));

	for (path, status, code) in cases {
		let output = map("inspect")
			.arg(&path)
			.output()
			.expect("reject selected input");
		assert_eq!(json(&output, status)["error"]["code"], code);
		assert!(!String::from_utf8_lossy(&output.stdout).contains("Unsupported.SC2Map"));
	}
}

#[cfg(unix)]
#[test]
fn inspect_refuses_a_symbolic_link_with_a_map_extension() {
	use std::os::unix::fs::symlink;

	let fixture = Fixture::new();
	let file = fixture.write("Real.SC2Map", ALPHA);
	let alias = fixture.path.join("Alias.SC2Map");
	symlink(file, &alias).expect("create synthetic link");
	let output = map("inspect")
		.arg(alias)
		.output()
		.expect("refuse symbolic link");
	assert_eq!(json(&output, 3)["error"]["code"], "invalid_map_selection");
}

#[test]
fn scan_reports_partial_results_and_never_writes_a_cache() {
	let fixture = Fixture::new();
	let alpha = fixture.write("Alpha.SC2Map", ALPHA);
	let rival = fixture.write("nested/Rival.s2ma", RIVAL);
	let rejected = fixture.write("Other.SC2Mod", UNSUPPORTED_ARCHIVE);
	let unrelated = fixture.write("Ignored.SC2Bank", b"not XML and unread by the map scan");
	let before = [&alpha, &rival, &rejected, &unrelated]
		.map(|path| fs::read(path).expect("read known fixture"));
	let output = map("scan")
		.arg("--root")
		.arg(&fixture.path)
		.output()
		.expect("scan synthetic archives without a cache");
	let report = json(&output, 0);
	assert_eq!(report["command"], "map_scan");
	let result = &report["result"];
	assert_eq!(result["incomplete"], true);
	let maps = result["maps"].as_array().expect("valid map results");
	assert_eq!(maps.len(), 3);
	assert!(maps.iter().any(|map| map["title"] == "Fixture Alpha"));
	assert!(maps.iter().any(|map| map["title"] == "Synthetic – alpha"));
	let issues = result["issues"].as_array().expect("typed diagnostics");
	assert_eq!(issues.len(), 1);
	assert_eq!(issues[0]["error"]["code"], "unsupported_map_archive");
	assert_eq!(
		[&alpha, &rival, &rejected, &unrelated]
			.map(|path| fs::read(path).expect("verify unchanged fixture")),
		before
	);
	assert_eq!(
		fs::read_dir(&fixture.path)
			.expect("inspect fixture directory")
			.count(),
		4
	);
}

#[test]
fn scan_uses_a_nonzero_status_when_every_selected_archive_fails() {
	let fixture = Fixture::new();
	let malformed = fixture.write("Malformed.SC2Map", b"MPQ\x1a");
	let output = map("scan")
		.arg("--root")
		.arg(&malformed)
		.output()
		.expect("report the archive failure");
	let report = json(&output, 3);
	assert_eq!(report["command"], "map_scan");
	assert_eq!(report["result"]["incomplete"], true);
	assert_eq!(report["result"]["maps"], serde_json::json!([]));
	assert_eq!(
		report["result"]["issues"][0]["error"]["code"],
		"invalid_map_archive"
	);
}
