use std::path::Path;

use sc2bank::Limits;
use serde::Serialize;
use starbank_fs::{FileSnapshot, StorageError, read_snapshot};
use starbank_paths::parse_signing_context;
use starbank_signature::SigningContext;

pub(crate) const BANK_EXTENSION: &str = "sc2bank";
pub(crate) const BANK_FILTER: &str = "StarCraft II bank";
const OPEN_TITLE: &str = "Open Bank";

// RFD prefixes each Linux extension with `*.` for GTK's case-sensitive shell glob.
#[cfg(target_os = "linux")]
const BANK_OPEN_EXTENSIONS: &[&str] = &["[sS][cC]2[bB][aA][nN][kK]"];
#[cfg(not(target_os = "linux"))]
const BANK_OPEN_EXTENSIONS: &[&str] = &[BANK_EXTENSION];

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpenError {
	InvalidName,
	UnsupportedType,
	TooLarge,
	Unreadable,
}

pub(crate) async fn pick_bank(window: &tauri::WebviewWindow) -> Option<rfd::FileHandle> {
	rfd::AsyncFileDialog::new()
		.set_parent(window)
		.set_title(OPEN_TITLE)
		.add_filter(BANK_FILTER, BANK_OPEN_EXTENSIONS)
		.pick_file()
		.await
}

pub(crate) fn bank_name(path: &Path) -> Result<String, OpenError> {
	if !path
		.extension()
		.is_some_and(|extension| extension.eq_ignore_ascii_case(BANK_EXTENSION))
	{
		return Err(OpenError::UnsupportedType);
	}
	path.file_name()
		.map(|name| name.to_string_lossy().into_owned())
		.ok_or(OpenError::InvalidName)
}

pub(crate) fn display_segments(path: &Path) -> Vec<String> {
	path.components()
		.map(|part| part.as_os_str().to_string_lossy().into_owned())
		.collect()
}

pub(crate) fn read_bank(path: &Path, limits: Limits) -> Result<(String, FileSnapshot), OpenError> {
	let name = bank_name(path)?;
	let snapshot = read_snapshot(path, limits.document_bytes).map_err(|error| match error {
		StorageError::TooLarge { .. } => OpenError::TooLarge,
		_ => OpenError::Unreadable,
	})?;
	Ok((name, snapshot))
}

pub(crate) fn destination_context(path: &Path) -> Option<SigningContext> {
	let hint = parse_signing_context(path)?;
	SigningContext::new(hint.author, hint.player, hint.bank_name).ok()
}

#[cfg(test)]
mod tests {
	#[cfg(unix)]
	use std::os::unix::fs as unix_fs;
	use std::{fs, io};

	use super::*;

	#[cfg(target_os = "linux")]
	#[test]
	#[ignore = "requires a GTK display under Xvfb"]
	fn gtk_open_filter_matches_all_ascii_casings() {
		gtk::init().expect("initialize GTK under Xvfb");
		let filter = gtk::FileFilter::new();
		for extension in BANK_OPEN_EXTENSIONS {
			filter.add_pattern(&format!("*.{extension}"));
		}
		let matches =
			|name: &str| filter.filter(&gtk::FileFilterInfo::new(None, None, Some(name), None));

		let letter_count = BANK_EXTENSION
			.bytes()
			.filter(u8::is_ascii_alphabetic)
			.count();
		for mut uppercase_bits in 0..(1_usize << letter_count) {
			let extension: String = BANK_EXTENSION
				.bytes()
				.map(|byte| {
					if !byte.is_ascii_alphabetic() {
						return char::from(byte);
					}
					let uppercase = uppercase_bits & 1 != 0;
					uppercase_bits >>= 1;
					char::from(if uppercase {
						byte.to_ascii_uppercase()
					} else {
						byte
					})
				})
				.collect();
			let name = format!("Fixture.{extension}");
			assert!(matches(&name), "GTK Open filter rejected {name:?}");
		}
		for name in [
			"Fixture.xml",
			"Fixture.SC2Bank.bak",
			"Fixture.sc2banks",
			"Fixture.SC3Bank",
			"FixtureSC2Bank",
			"Fixture.ſc2bank",
		] {
			assert!(!matches(name), "GTK Open filter accepted {name:?}");
		}
	}

	#[test]
	fn reads_named_regular_file_within_bank_limit() -> io::Result<()> {
		let folder = tempfile::tempdir()?;
		let path = folder.path().canonicalize()?.join("Synthetic.Sc2Bank");
		let expected = b"<Bank version=\"1\"/>";
		fs::write(&path, expected)?;
		let (name, bytes) = read_bank(&path, Limits::default()).expect("fixture accepted");
		assert_eq!(name, "Synthetic.Sc2Bank");
		assert_eq!(bytes.bytes(), expected);
		let limits = Limits {
			document_bytes: expected.len() - 1,
			..Limits::default()
		};
		assert!(matches!(read_bank(&path, limits), Err(OpenError::TooLarge)));
		Ok(())
	}

	#[test]
	fn rejects_wrong_extension_and_directory() -> io::Result<()> {
		let folder = tempfile::tempdir()?;
		let wrong_type = folder.path().canonicalize()?.join("Not a bank.xml");
		fs::write(&wrong_type, b"<Bank/>")?;
		assert!(matches!(
			read_bank(&wrong_type, Limits::default()),
			Err(OpenError::UnsupportedType)
		));
		let directory = folder.path().canonicalize()?.join("folder.SC2Bank");
		fs::create_dir(&directory)?;
		assert!(read_bank(&directory, Limits::default()).is_err());
		Ok(())
	}

	#[cfg(unix)]
	#[test]
	fn refuses_a_symlink_even_when_target_is_a_bank() -> io::Result<()> {
		let folder = tempfile::tempdir()?;
		let bank = folder.path().canonicalize()?.join("real.SC2Bank");
		let link = folder.path().canonicalize()?.join("linked.SC2Bank");
		fs::write(&bank, b"<Bank/>")?;
		unix_fs::symlink(&bank, &link)?;
		assert!(read_bank(&link, Limits::default()).is_err());
		Ok(())
	}
}
