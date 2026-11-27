use std::path::{Path, PathBuf};

use serde_json::{Value, json as object};

use crate::cli::{Fixture, bank_command, json};

pub fn inspect(path: &Path) -> Value {
	json(
		&bank_command("inspect", path)
			.output()
			.expect("inspect input"),
		0,
	)
}

pub fn digest(path: &Path) -> String {
	inspect(path)["result"]["sha256"]
		.as_str()
		.expect("digest")
		.to_owned()
}

pub fn edit_patch(operation: Value, digest: &str, fixture: &Fixture) -> PathBuf {
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
