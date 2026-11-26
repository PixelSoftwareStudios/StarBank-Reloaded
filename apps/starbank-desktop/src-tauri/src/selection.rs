use std::path::Path;

use sc2bank::Limits;
use serde::Serialize;
use starbank_fs::{FileSnapshot, StorageError, read_snapshot};
use starbank_paths::parse_signing_context;
use starbank_signature::SigningContext;

pub(crate) const BANK_EXTENSION: &str = "sc2bank";
pub(crate) const BANK_FILTER: &str = "StarCraft II bank";
const OPEN_TITLE: &str = "Open Bank";

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
		.add_filter(BANK_FILTER, &[BANK_EXTENSION])
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
