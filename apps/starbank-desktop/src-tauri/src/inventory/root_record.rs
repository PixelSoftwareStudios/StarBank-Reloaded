use std::{
	fs,
	io::{self, Write},
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::authority::{DirectoryIdentity, is_native_disk_path};

const RECORD_VERSION: u32 = 1;
const MAX_RECORD_BYTES: usize = 256 * 1024;
const MAX_PATH_UNITS: usize = 32_768;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ManualRoot {
	pub(super) path: PathBuf,
	pub(super) identity: Option<DirectoryIdentity>,
}

pub(super) struct RootRecordStore(PathBuf);

impl RootRecordStore {
	pub(super) fn new(file: PathBuf) -> Self {
		Self(file)
	}

	pub(super) fn load(&self) -> io::Result<Option<ManualRoot>> {
		let Some(snapshot) = read_record(&self.0)? else {
			return Ok(None);
		};
		let record: RootRecord = serde_json::from_slice(snapshot.bytes())?;
		if record.version != RECORD_VERSION || record.platform != std::env::consts::OS {
			return Err(invalid_record());
		}
		record.manual.map(StoredRoot::into_root).transpose()
	}

	pub(super) fn save(&self, manual: Option<&ManualRoot>) -> io::Result<()> {
		let parent = self.0.parent().ok_or_else(invalid_record)?;
		fs::create_dir_all(parent)?;
		let previous = read_record(&self.0)?;
		let record = RootRecord {
			version: RECORD_VERSION,
			platform: std::env::consts::OS.to_owned(),
			manual: manual.map(StoredRoot::from_root).transpose()?,
		};
		let bytes = serde_json::to_vec(&record)?;
		if bytes.len() > MAX_RECORD_BYTES {
			return Err(invalid_record());
		}
		let mut staged = tempfile::NamedTempFile::new_in(parent)?;
		staged.write_all(&bytes)?;
		staged.as_file().sync_all()?;
		if let Some(previous) = previous {
			if read_record(&self.0)?
				.is_none_or(|current| current.fingerprint() != previous.fingerprint())
			{
				return Err(io::Error::new(
					io::ErrorKind::ResourceBusy,
					"manual authorization record changed",
				));
			}
			staged.persist(&self.0).map_err(|error| error.error)?;
		} else {
			staged
				.persist_noclobber(&self.0)
				.map_err(|error| error.error)?;
		}
		if read_record(&self.0)?.is_none_or(|current| current.bytes() != bytes) {
			return Err(io::Error::other(
				"manual authorization record verification failed",
			));
		}
		Ok(())
	}
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RootRecord {
	version: u32,
	platform: String,
	manual: Option<StoredRoot>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredRoot {
	native_path: Vec<PathUnit>,
	identity: DirectoryIdentity,
}

impl StoredRoot {
	fn from_root(manual: &ManualRoot) -> io::Result<Self> {
		let native_path = encode_path(&manual.path);
		if native_path.is_empty() || native_path.len() > MAX_PATH_UNITS {
			return Err(invalid_record());
		}
		Ok(Self {
			native_path,
			identity: manual.identity.ok_or_else(invalid_record)?,
		})
	}

	fn into_root(self) -> io::Result<ManualRoot> {
		if self.native_path.is_empty()
			|| self.native_path.len() > MAX_PATH_UNITS
			|| self.native_path.contains(&0)
		{
			return Err(invalid_record());
		}
		let path = decode_path(self.native_path);
		if !is_native_disk_path(&path) {
			return Err(invalid_record());
		}
		Ok(ManualRoot {
			path,
			identity: Some(self.identity),
		})
	}
}

fn read_record(path: &Path) -> io::Result<Option<starbank_fs::FileSnapshot>> {
	match starbank_fs::read_snapshot(path, MAX_RECORD_BYTES) {
		Ok(snapshot) => Ok(Some(snapshot)),
		Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
		Err(error) => Err(io::Error::new(
			error.kind(),
			"manual authorization record could not be read",
		)),
	}
}

fn invalid_record() -> io::Error {
	io::Error::new(
		io::ErrorKind::InvalidData,
		"manual authorization record is invalid",
	)
}

#[cfg(unix)]
type PathUnit = u8;
#[cfg(windows)]
type PathUnit = u16;

#[cfg(unix)]
fn encode_path(path: &Path) -> Vec<PathUnit> {
	use std::os::unix::ffi::OsStrExt as _;
	path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn encode_path(path: &Path) -> Vec<PathUnit> {
	use std::os::windows::ffi::OsStrExt as _;
	path.as_os_str().encode_wide().collect()
}

#[cfg(unix)]
fn decode_path(units: Vec<PathUnit>) -> PathBuf {
	use std::os::unix::ffi::OsStringExt as _;
	std::ffi::OsString::from_vec(units).into()
}

#[cfg(windows)]
fn decode_path(units: Vec<PathUnit>) -> PathBuf {
	use std::os::windows::ffi::OsStringExt as _;
	std::ffi::OsString::from_wide(&units).into()
}
