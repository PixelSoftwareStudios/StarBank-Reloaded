use super::{Fixture, command, fs, json};

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
fn discover_distinguishes_invalid_selections_missing_paths_and_scan_limits() {
	// One more directory level than the discovery depth ceiling.
	const TOO_DEEP: usize = 65;
	let fixture = Fixture::new();
	let invalid = fixture.write("private-invalid.txt", b"not a bank");
	let missing = fixture.path.join("private-missing.sc2bank");
	let banks = fixture.path.join("private-deep").join("Banks");
	let mut deepest = banks.clone();
	for _ in 0..TOO_DEEP {
		deepest.push("d");
	}
	fs::create_dir_all(deepest).expect("create bank scope beyond the scan depth limit");

	for (root, status, code) in [
		(invalid, 3, "invalid_bank_selection"),
		(missing, 4, "discovery_failed"),
		(banks, 3, "input_limit"),
	] {
		let output = command()
			.args(["discover", "--root"])
			.arg(root)
			.arg("--json")
			.output()
			.expect("run discovery for an invalid, missing, or excessive scope");
		assert_eq!(json(&output, status)["error"]["code"], code);
		assert!(!String::from_utf8_lossy(&output.stdout).contains("private-"));
	}
}
