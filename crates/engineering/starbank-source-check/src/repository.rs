use std::ffi::OsString;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::layout::check_source;

const RUST_PATHSPEC: &str = "*.rs";
const ARCHIVE_EXCLUSION: &str = ":(exclude,top)original/";
// Publisher formatting is preserved and checked by the vendored byte-integrity gate.
const GLIB_VENDOR_EXCLUSION: &str = ":(exclude,top)crates/vendor/glib/";
const RFD_VENDOR_EXCLUSION: &str = ":(exclude,top)crates/vendor/rfd/";
const GIT_SYMLINK_MODE: &[u8] = b"120000 ";

pub(crate) fn run() -> ExitCode {
	match check_repository() {
		Ok(true) => ExitCode::SUCCESS,
		Ok(false) => ExitCode::FAILURE,
		Err(error) => {
			eprintln!("Rust source check: {error}");
			ExitCode::FAILURE
		}
	}
}

fn check_repository() -> io::Result<bool> {
	if !git(&["rev-parse", "--show-prefix"])?
		.trim_ascii()
		.is_empty()
	{
		return Err(io::Error::other("run from the repository root"));
	}

	let tracked = list_sources(&["--stage"])?;
	let untracked = list_sources(&["--others", "--exclude-standard"])?;
	let mut valid = true;
	let mut checked = 0;
	for record in records(&tracked) {
		let (path, is_symlink) = tracked_path(record)?;
		valid &= check_path(&path, is_symlink, &mut checked);
	}
	for record in records(&untracked) {
		let path = parse_path(record)?;
		valid &= check_path(&path, false, &mut checked);
	}
	if valid && checked > 0 {
		println!("Checked {checked} Rust source files");
	} else if valid {
		return Err(io::Error::other("no Rust source files to check"));
	}
	Ok(valid)
}

fn list_sources(options: &[&str]) -> io::Result<Vec<u8>> {
	let mut args = vec!["ls-files", "-z"];
	args.extend_from_slice(options);
	args.extend_from_slice(&[
		"--",
		RUST_PATHSPEC,
		ARCHIVE_EXCLUSION,
		GLIB_VENDOR_EXCLUSION,
		RFD_VENDOR_EXCLUSION,
	]);
	git(&args)
}

fn git(args: &[&str]) -> io::Result<Vec<u8>> {
	let output = Command::new("git").args(args).output()?;
	if !output.status.success() {
		return Err(io::Error::other(format!(
			"git {} failed: {}",
			args.join(" "),
			String::from_utf8_lossy(&output.stderr).trim_end()
		)));
	}
	Ok(output.stdout)
}

fn records(output: &[u8]) -> impl Iterator<Item = &[u8]> {
	output
		.split(|byte| *byte == 0)
		.filter(|record| !record.is_empty())
}

fn tracked_path(record: &[u8]) -> io::Result<(PathBuf, bool)> {
	let separator = record
		.iter()
		.position(|byte| *byte == b'\t')
		.ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid git index record"))?;
	let is_symlink = record[..separator].starts_with(GIT_SYMLINK_MODE);
	Ok((parse_path(&record[separator + 1..])?, is_symlink))
}

fn parse_path(bytes: &[u8]) -> io::Result<PathBuf> {
	#[cfg(unix)]
	let path = {
		use std::os::unix::ffi::OsStringExt;

		OsString::from_vec(bytes.to_vec())
	};
	#[cfg(not(unix))]
	let path = std::str::from_utf8(bytes)
		.map(OsString::from)
		.map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;

	let path = PathBuf::from(path);
	if path.as_os_str().is_empty()
		|| path
			.components()
			.any(|component| !matches!(component, Component::Normal(_)))
	{
		return Err(io::Error::new(
			ErrorKind::InvalidData,
			format!("invalid checkout path {}", path.display()),
		));
	}
	Ok(path)
}

fn check_path(path: &Path, is_symlink: bool, checked: &mut usize) -> bool {
	if is_symlink {
		eprintln!("{}: Rust source is tracked as a symlink", path.display());
		return false;
	}
	let source = match read_source(path) {
		Ok(Some(source)) => source,
		Ok(None) => return true,
		Err(error) => {
			eprintln!("{}: {error}", path.display());
			return false;
		}
	};
	*checked += 1;
	let errors = check_source(path, &source);
	for error in &errors {
		eprintln!("{}: {error}", path.display());
	}
	errors.is_empty()
}

fn read_source(path: &Path) -> io::Result<Option<String>> {
	let mut current = PathBuf::new();
	for part in path.components() {
		current.push(part);
		let metadata = match fs::symlink_metadata(&current) {
			Ok(metadata) => metadata,
			Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
			Err(error) => return Err(error),
		};
		if metadata.file_type().is_symlink() {
			return Err(io::Error::other(format!(
				"Rust source path traverses symlink {}",
				current.display()
			)));
		}
	}
	if !fs::metadata(path)?.is_file() {
		return Err(io::Error::other("Rust source is not a regular file"));
	}
	fs::read_to_string(path).map(Some)
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use super::{records, tracked_path};

	#[test]
	fn handles_git_modes_and_whitespace_without_decoding_quoted_paths() {
		let input =
			b"100644 object 0\tcrates/my files/leaf\tcode.rs\x00120000 object 0\talias.rs\x00";
		let mut entries = records(input);
		let (source, is_symlink) = tracked_path(entries.next().unwrap()).unwrap();
		assert_eq!(source, Path::new("crates/my files/leaf\tcode.rs"));
		assert!(!is_symlink);
		let (source, is_symlink) = tracked_path(entries.next().unwrap()).unwrap();
		assert_eq!(source, Path::new("alias.rs"));
		assert!(is_symlink);
		assert!(entries.next().is_none());
	}

	#[test]
	fn rejects_paths_that_can_leave_the_checkout() {
		for path in [
			b"100644 object 0\t../outside.rs".as_slice(),
			b"100644 object 0\t/outside.rs".as_slice(),
			b"invalid record".as_slice(),
		] {
			assert!(tracked_path(path).is_err());
		}
	}
}
