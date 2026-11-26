use std::{
	fs,
	path::{Path, PathBuf},
	process::{Command, Output},
};

use serde_json::Value;
use tempfile::TempDir;

pub const SOURCE: &str = r#"<Bank version="1">
	<Section name="Player">
		<Key name="Points"><Value fixed="9007199254740993.050"/></Key>
		<Key name="Points"><Value int="00065"/><Label string="A &amp; B"/></Key>
	</Section>
</Bank>"#;

pub struct Fixture {
	_directory: TempDir,
	pub path: PathBuf,
}

impl Fixture {
	pub fn new() -> Self {
		let directory = tempfile::tempdir().expect("create synthetic CLI fixture directory");
		let path = directory
			.path()
			.canonicalize()
			.expect("resolve fixture directory");
		Self {
			_directory: directory,
			path,
		}
	}

	pub fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
		let path = self.path.join(name);
		fs::create_dir_all(path.parent().expect("fixture file parent"))
			.expect("create fixture parents");
		fs::write(&path, bytes).expect("write synthetic CLI fixture");
		path
	}
}

pub fn command() -> Command {
	Command::new(env!("CARGO_BIN_EXE_starbank"))
}

pub fn json(output: &Output, code: i32) -> Value {
	assert_eq!(
		output.status.code(),
		Some(code),
		"stdout: {}; stderr: {}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(
		output.stderr.is_empty(),
		"JSON output should have empty stderr"
	);
	let body: Value = serde_json::from_slice(&output.stdout).expect("parse one JSON document");
	assert_eq!(body["schema_version"], 1);
	body
}

pub fn bank_command(name: &str, path: &Path) -> Command {
	let mut command = command();
	command.arg(name).arg(path).arg("--json");
	command
}
