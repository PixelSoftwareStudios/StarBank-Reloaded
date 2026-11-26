use std::{
	fs,
	path::{Component, Path, PathBuf},
	sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::selection::BANK_EXTENSION;

use super::{InventoryError, InventoryReason};

#[derive(Debug)]
pub(super) struct AuthorizedRoot {
	path: PathBuf,
	identity: DirectoryIdentity,
}

impl AuthorizedRoot {
	pub(super) fn capture(path: &Path) -> Result<Arc<Self>, InventoryReason> {
		let identity = DirectoryIdentity::read(path).ok_or(InventoryReason::Unreadable)?;
		Ok(Arc::new(Self {
			path: path.to_owned(),
			identity,
		}))
	}

	pub(super) fn path(&self) -> &Path {
		&self.path
	}

	pub(super) fn identity(&self) -> DirectoryIdentity {
		self.identity
	}
}

#[derive(Clone, Debug)]
pub(crate) struct BankAuthority {
	path: PathBuf,
	root: Arc<AuthorizedRoot>,
}

impl BankAuthority {
	pub(super) fn new(path: &Path, root: Arc<AuthorizedRoot>) -> Option<Self> {
		if !path.starts_with(root.path())
			|| !is_native_disk_path(path)
			|| !path
				.extension()
				.is_some_and(|extension| extension.eq_ignore_ascii_case(BANK_EXTENSION))
		{
			return None;
		}
		Some(Self {
			path: path.to_owned(),
			root,
		})
	}

	pub(crate) fn path(&self) -> &Path {
		&self.path
	}

	pub(crate) fn validate(&self) -> Result<(), InventoryError> {
		let Some(parent) = self.path.parent() else {
			return Err(InventoryError::StaleBank);
		};
		if !ordinary_directories(parent)
			|| DirectoryIdentity::read(self.root.path()) != Some(self.root.identity)
			|| checked_metadata(&self.path).is_none_or(|metadata| !metadata.is_file())
		{
			return Err(InventoryError::StaleBank);
		}
		Ok(())
	}
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(super) struct DirectoryIdentity {
	#[cfg(unix)]
	device: u64,
	#[cfg(unix)]
	inode: u64,
	#[cfg(not(unix))]
	created: std::time::SystemTime,
}

impl DirectoryIdentity {
	pub(super) fn read(path: &Path) -> Option<Self> {
		let metadata = checked_metadata(path)?;
		if !metadata.is_dir() {
			return None;
		}
		#[cfg(unix)]
		{
			use std::os::unix::fs::MetadataExt;
			Some(Self {
				device: metadata.dev(),
				inode: metadata.ino(),
			})
		}
		#[cfg(not(unix))]
		{
			Some(Self {
				created: metadata.created().ok()?,
			})
		}
	}
}

fn checked_metadata(path: &Path) -> Option<fs::Metadata> {
	let metadata = fs::symlink_metadata(path).ok()?;
	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt as _;
		use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
		if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
			return None;
		}
	}
	Some(metadata)
}

pub(super) fn ordinary_directories(path: &Path) -> bool {
	let mut ancestor = PathBuf::new();
	for component in path.components() {
		ancestor.push(component);
		if matches!(component, Component::Normal(_))
			&& checked_metadata(&ancestor).is_none_or(|metadata| !metadata.is_dir())
		{
			return false;
		}
	}
	true
}

pub(super) fn is_native_disk_path(path: &Path) -> bool {
	path.is_absolute()
		&& path.components().all(|part| part != Component::ParentDir)
		&& local_prefix(path)
}

#[cfg(windows)]
fn local_prefix(path: &Path) -> bool {
	use std::path::Prefix;
	matches!(path.components().next(),
		Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
	)
}

#[cfg(not(windows))]
fn local_prefix(_path: &Path) -> bool {
	true
}
