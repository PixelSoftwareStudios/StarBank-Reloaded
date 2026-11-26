use std::{
	fs::{self, File, OpenOptions, TryLockError},
	io,
	path::{Path, PathBuf},
};

use starbank_fs::{FileSnapshot, read_snapshot};

use super::{
	SETTINGS_FILE_NAME, SettingsIssue, SettingsSnapshot,
	schema::{AppSettings, LegacyImport, SettingChange},
};

const MAX_SETTINGS_BYTES: usize = 64 * 1024;
const SETTINGS_LOCK_NAME: &str = ".settings.lock";
#[cfg(not(target_os = "macos"))]
const SETTINGS_STAGE_PREFIX: &str = ".settings-stage-";

/// A preference store rooted at one app-owned directory.
#[derive(Debug)]
pub(crate) struct SettingsStore {
	directory: PathBuf,
	legacy: PathBuf,
}

impl SettingsStore {
	pub(super) fn new(directory: PathBuf, legacy: PathBuf) -> Self {
		Self { directory, legacy }
	}

	pub(super) fn load(&self) -> Result<SettingsSnapshot, SettingsIssue> {
		if let Some(saved) = self.read_saved()? {
			return Ok(self.snapshot(saved));
		}
		Ok(self
			.read_legacy()?
			.map_or_else(default_snapshot, |legacy| SettingsSnapshot {
				settings: legacy.settings,
				legacy_root_pending: legacy.root.is_some(),
			}))
	}

	pub(super) fn load_or_migrate(&self) -> Result<SettingsSnapshot, SettingsIssue> {
		if let Some(saved) = self.read_saved()? {
			return Ok(self.snapshot(saved));
		}
		if self.read_legacy()?.is_none() {
			return Ok(default_snapshot());
		}
		let _lease = match self.lock() {
			Ok(lease) => lease,
			Err(SettingsIssue::SettingsChanged) => return self.load(),
			Err(error) => return Err(error),
		};
		if let Some(saved) = self.read_saved()? {
			return Ok(self.snapshot(saved));
		}
		let Some(legacy) = self.read_legacy()? else {
			return Ok(default_snapshot());
		};
		self.publish(legacy.settings, None)?;
		Ok(self.snapshot(legacy.settings))
	}

	pub(super) fn change(&self, change: SettingChange) -> Result<SettingsSnapshot, SettingsIssue> {
		let _lease = self.lock()?;
		let original = self.read_current()?;
		let mut settings = match original.as_ref() {
			Some(snapshot) => AppSettings::decode(snapshot.bytes())?,
			None => self
				.read_legacy()?
				.map_or_else(AppSettings::default, |legacy| legacy.settings),
		};
		settings.apply(change)?;
		self.publish(settings, original.as_ref())?;
		Ok(self.snapshot(settings))
	}

	pub(super) fn recover(&self) -> Result<SettingsSnapshot, SettingsIssue> {
		let _lease = self.lock()?;
		let original = self.read_current()?;
		let current = match original.as_ref() {
			Some(snapshot) => AppSettings::decode(snapshot.bytes()),
			None => self
				.read_legacy()
				.map(|legacy| legacy.map_or_else(AppSettings::default, |legacy| legacy.settings)),
		};
		match current {
			Err(SettingsIssue::InvalidJson | SettingsIssue::InvalidSettings) => (),
			Ok(_) => return Err(SettingsIssue::SettingsChanged),
			Err(error) => return Err(error),
		}

		let settings = AppSettings::default();
		self.publish(settings, original.as_ref())?;
		Ok(self.snapshot(settings))
	}

	pub(super) fn read_legacy(&self) -> Result<Option<LegacyImport>, SettingsIssue> {
		read_optional(&self.legacy)?
			.map(|snapshot| LegacyImport::decode(snapshot.bytes()))
			.transpose()
	}

	fn snapshot(&self, settings: AppSettings) -> SettingsSnapshot {
		SettingsSnapshot {
			settings,
			legacy_root_pending: matches!(self.legacy_root_override(), Ok(Some(_))),
		}
	}

	fn read_saved(&self) -> Result<Option<AppSettings>, SettingsIssue> {
		self.read_current()?
			.map(|snapshot| AppSettings::decode(snapshot.bytes()))
			.transpose()
	}

	fn read_current(&self) -> Result<Option<FileSnapshot>, SettingsIssue> {
		read_optional(&self.path())
	}

	fn path(&self) -> PathBuf {
		self.directory.join(SETTINGS_FILE_NAME)
	}

	fn lock(&self) -> Result<File, SettingsIssue> {
		fs::create_dir_all(&self.directory).map_err(|_| SettingsIssue::StorageUnavailable)?;
		let mut options = OpenOptions::new();
		options.read(true).write(true).create(true).truncate(false);
		protect_lock_path(&mut options);
		let lock = options
			.open(self.directory.join(SETTINGS_LOCK_NAME))
			.map_err(|_| SettingsIssue::StorageUnavailable)?;
		if !lock
			.metadata()
			.map_err(|_| SettingsIssue::StorageUnavailable)?
			.is_file()
		{
			return Err(SettingsIssue::StorageUnavailable);
		}

		lock.try_lock().map_err(|error| match error {
			TryLockError::WouldBlock => SettingsIssue::SettingsChanged,
			TryLockError::Error(_) => SettingsIssue::StorageUnavailable,
		})?;
		Ok(lock)
	}

	fn publish(
		&self,
		settings: AppSettings,
		original: Option<&FileSnapshot>,
	) -> Result<(), SettingsIssue> {
		let mut bytes =
			serde_json::to_vec_pretty(&settings).map_err(|_| SettingsIssue::InvalidSettings)?;
		bytes.push(b'\n');
		if bytes.len() > MAX_SETTINGS_BYTES {
			return Err(SettingsIssue::SettingsTooLarge);
		}

		self.publish_bytes(&bytes, original)
	}

	#[cfg(target_os = "macos")]
	fn publish_bytes(
		&self,
		bytes: &[u8],
		original: Option<&FileSnapshot>,
	) -> Result<(), SettingsIssue> {
		use starbank_fs::{CommitIntent, CommitOutcome};

		let path = self.path();
		let intent = original.map_or(CommitIntent::CreateNew, |snapshot| {
			CommitIntent::ReplaceExpected(snapshot.fingerprint())
		});
		match starbank_fs::commit(&path, bytes, intent, MAX_SETTINGS_BYTES) {
			CommitOutcome::CommittedVerified(receipt) => {
				// The preserved sibling belongs to the app config transaction.
				if let Some(previous) = receipt.recovery() {
					fs::remove_file(previous.path())
						.map_err(|_| SettingsIssue::StorageUnavailable)?;
				}
				Ok(())
			}
			CommitOutcome::RejectedUnchanged(error) => Err(match error.kind() {
				io::ErrorKind::AlreadyExists | io::ErrorKind::ResourceBusy => {
					SettingsIssue::SettingsChanged
				}
				_ => SettingsIssue::StorageUnavailable,
			}),
			CommitOutcome::CommitUncertain(_) => Err(SettingsIssue::StorageUnavailable),
		}
	}

	#[cfg(not(target_os = "macos"))]
	fn publish_bytes(
		&self,
		bytes: &[u8],
		original: Option<&FileSnapshot>,
	) -> Result<(), SettingsIssue> {
		use std::io::Write as _;

		let path = self.path();
		let mut stage = tempfile::Builder::new()
			.prefix(SETTINGS_STAGE_PREFIX)
			.tempfile_in(&self.directory)
			.map_err(|_| SettingsIssue::StorageUnavailable)?;
		stage
			.write_all(bytes)
			.map_err(|_| SettingsIssue::StorageUnavailable)?;
		if original.is_some() {
			let metadata =
				fs::symlink_metadata(&path).map_err(|_| SettingsIssue::SettingsChanged)?;
			if !metadata.is_file() || metadata.permissions().readonly() {
				return Err(SettingsIssue::StorageUnavailable);
			}
			stage
				.as_file()
				.set_permissions(metadata.permissions())
				.map_err(|_| SettingsIssue::StorageUnavailable)?;
		}
		stage
			.as_file()
			.sync_all()
			.map_err(|_| SettingsIssue::StorageUnavailable)?;

		let current = self.read_current()?;
		if original.map(FileSnapshot::fingerprint)
			!= current.as_ref().map(FileSnapshot::fingerprint)
		{
			return Err(SettingsIssue::SettingsChanged);
		}
		if current.is_some() {
			stage
				.persist(path)
				.map_err(|_| SettingsIssue::StorageUnavailable)?;
		} else {
			stage.persist_noclobber(path).map_err(|failure| {
				if failure.error.kind() == io::ErrorKind::AlreadyExists {
					SettingsIssue::SettingsChanged
				} else {
					SettingsIssue::StorageUnavailable
				}
			})?;
		}
		sync_directory(&self.directory)
	}
}

fn default_snapshot() -> SettingsSnapshot {
	SettingsSnapshot {
		settings: AppSettings::default(),
		legacy_root_pending: false,
	}
}

fn read_optional(path: &Path) -> Result<Option<FileSnapshot>, SettingsIssue> {
	match read_snapshot(path, MAX_SETTINGS_BYTES) {
		Ok(snapshot) => Ok(Some(snapshot)),
		Err(error) => match error.kind() {
			io::ErrorKind::NotFound => Ok(None),
			io::ErrorKind::FileTooLarge => Err(SettingsIssue::SettingsTooLarge),
			io::ErrorKind::ResourceBusy => Err(SettingsIssue::SettingsChanged),
			_ => Err(SettingsIssue::StorageUnavailable),
		},
	}
}

#[cfg(unix)]
fn protect_lock_path(options: &mut OpenOptions) {
	use std::os::unix::fs::OpenOptionsExt as _;

	options
		.mode(0o600)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
}

#[cfg(windows)]
fn protect_lock_path(options: &mut OpenOptions) {
	use std::os::windows::fs::OpenOptionsExt as _;
	use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

	options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(all(unix, not(target_os = "macos")))]
fn sync_directory(directory: &Path) -> Result<(), SettingsIssue> {
	File::open(directory)
		.and_then(|handle| handle.sync_all())
		.map_err(|_| SettingsIssue::StorageUnavailable)
}

#[cfg(windows)]
fn sync_directory(_directory: &Path) -> Result<(), SettingsIssue> {
	Ok(())
}
