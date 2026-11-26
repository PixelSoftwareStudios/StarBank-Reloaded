use std::{fs::OpenOptions, io::Read, path::Path};

use sc2bank::Limits;
use serde::Serialize;

pub(crate) const BANK_EXTENSION: &str = "sc2bank";
pub(crate) const BANK_FILTER: &str = "StarCraft II bank";
const OPEN_TITLE: &str = "Open Bank";

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpenError {
	InvalidName,
	UnsupportedType,
	NotRegular,
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

pub(crate) fn read_bank(path: &Path, limits: Limits) -> Result<(String, Vec<u8>), OpenError> {
	let name = bank_name(path)?;
	let file = bank_open_options()
		.open(path)
		.map_err(|_| OpenError::Unreadable)?;
	let metadata = file.metadata().map_err(|_| OpenError::Unreadable)?;
	if !is_regular(&metadata) {
		return Err(OpenError::NotRegular);
	}
	let limit = u64::try_from(limits.document_bytes).map_err(|_| OpenError::TooLarge)?;
	if metadata.len() > limit {
		return Err(OpenError::TooLarge);
	}
	let mut bytes = Vec::new();
	file.take(limit.saturating_add(1))
		.read_to_end(&mut bytes)
		.map_err(|_| OpenError::Unreadable)?;
	if bytes.len() > limits.document_bytes {
		return Err(OpenError::TooLarge);
	}
	Ok((name, bytes))
}

fn bank_open_options() -> OpenOptions {
	let mut options = OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt as _;

		// A substituted symlink must not resolve, and a pipe must not block the opener.
		options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
	}
	#[cfg(windows)]
	{
		use std::os::windows::fs::OpenOptionsExt as _;
		use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

		options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
	}
	options
}

fn is_regular(metadata: &std::fs::Metadata) -> bool {
	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt as _;
		use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

		if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
			return false;
		}
	}
	metadata.is_file()
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
		let path = folder.path().join("Synthetic.Sc2Bank");
		let expected = b"<Bank version=\"1\"/>";
		fs::write(&path, expected)?;
		let (name, bytes) = read_bank(&path, Limits::default()).expect("fixture accepted");
		assert_eq!(name, "Synthetic.Sc2Bank");
		assert_eq!(bytes, expected);
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
		let wrong_type = folder.path().join("Not a bank.xml");
		fs::write(&wrong_type, b"<Bank/>")?;
		assert!(matches!(
			read_bank(&wrong_type, Limits::default()),
			Err(OpenError::UnsupportedType)
		));
		let directory = folder.path().join("folder.SC2Bank");
		fs::create_dir(&directory)?;
		assert!(read_bank(&directory, Limits::default()).is_err());
		Ok(())
	}

	#[cfg(unix)]
	#[test]
	fn refuses_a_symlink_even_when_target_is_a_bank() -> io::Result<()> {
		let folder = tempfile::tempdir()?;
		let bank = folder.path().join("real.SC2Bank");
		let link = folder.path().join("linked.SC2Bank");
		fs::write(&bank, b"<Bank/>")?;
		unix_fs::symlink(&bank, &link)?;
		assert!(read_bank(&link, Limits::default()).is_err());
		Ok(())
	}
}
