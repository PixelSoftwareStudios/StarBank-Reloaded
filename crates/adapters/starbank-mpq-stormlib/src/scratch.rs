use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};

use rustix::fs::{Mode, OFlags, fstatfs, open};
use tempfile::TempDir;

use crate::{AppendError, ScratchOperation};

const SCRATCH_ROOT: &str = "/private/tmp";
const DIRECTORY_PREFIX: &str = "starbank-mpq-";
const SCRATCH_NAME: &str = "candidate.mpq";
const LOCAL_APFS: &[u8] = b"apfs\0";
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;
const PUBLIC_STICKY_DIRECTORY_MODE: u32 = 0o1777;
const ACCESS_MODE_MASK: u32 = 0o7777;
const ROOT_UID: u32 = 0;

pub(super) struct Scratch {
	directory: TempDir,
	file: File,
	path: CString,
}

impl Scratch {
	pub(super) fn create(source: &[u8]) -> Result<Self, AppendError> {
		check_scratch_root()?;
		let directory = tempfile::Builder::new()
			.prefix(DIRECTORY_PREFIX)
			.permissions(Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
			.tempdir_in(SCRATCH_ROOT)
			.map_err(|error| AppendError::scratch(ScratchOperation::CreateDirectory, error))?;
		let path = directory.path().join(SCRATCH_NAME);
		let native_path = native_path(&path)?;
		let mut file = OpenOptions::new()
			.read(true)
			.write(true)
			.create_new(true)
			.mode(PRIVATE_FILE_MODE)
			.open(&path)
			.map_err(|error| AppendError::scratch(ScratchOperation::CreateFile, error))?;
		file.write_all(source)
			.map_err(|error| AppendError::scratch(ScratchOperation::WriteSource, error))?;
		Ok(Self {
			directory,
			file,
			path: native_path,
		})
	}

	pub(super) fn path(&self) -> &CStr {
		&self.path
	}

	pub(super) fn read_candidate(&mut self, max_bytes: u64) -> Result<Vec<u8>, AppendError> {
		let map_io = |error| AppendError::scratch(ScratchOperation::ReadCandidate, error);
		let metadata = self.file.metadata().map_err(map_io)?;
		if metadata.len() > max_bytes {
			return Err(AppendError::CandidateTooLarge);
		}
		self.file.seek(SeekFrom::Start(0)).map_err(map_io)?;
		let length = usize::try_from(metadata.len()).map_err(|_| AppendError::CandidateTooLarge)?;
		let mut bytes = Vec::new();
		bytes
			.try_reserve_exact(length)
			.map_err(|error| map_io(io::Error::other(error)))?;
		bytes.resize(length, 0);
		self.file.read_exact(&mut bytes).map_err(map_io)?;
		if self.file.read(&mut [0]).map_err(map_io)? != 0 {
			return Err(AppendError::CandidateChanged);
		}
		Ok(bytes)
	}

	pub(super) fn remove(self) -> Result<(), AppendError> {
		let Self {
			file, directory, ..
		} = self;
		drop(file);
		directory
			.close()
			.map_err(|error| AppendError::scratch(ScratchOperation::RemoveDirectory, error))
	}
}

fn check_scratch_root() -> Result<(), AppendError> {
	let root = open(
		SCRATCH_ROOT,
		OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
		Mode::empty(),
	)
	.map(File::from)
	.map_err(|error| AppendError::scratch(ScratchOperation::CheckVolume, error))?;
	let metadata = root
		.metadata()
		.map_err(|error| AppendError::scratch(ScratchOperation::CheckVolume, error))?;
	let volume = fstatfs(&root)
		.map_err(|error| AppendError::scratch(ScratchOperation::CheckVolume, error))?;
	let filesystem = volume.f_fstypename.iter().map(|byte| byte.to_ne_bytes()[0]);
	if metadata.uid() != ROOT_UID
		|| metadata.mode() & ACCESS_MODE_MASK != PUBLIC_STICKY_DIRECTORY_MODE
		|| volume.f_flags & libc::MNT_LOCAL as u32 == 0
		|| !filesystem
			.take(LOCAL_APFS.len())
			.eq(LOCAL_APFS.iter().copied())
	{
		return Err(AppendError::UnsupportedScratch);
	}
	Ok(())
}

fn native_path(path: &Path) -> Result<CString, AppendError> {
	// StormLib recognizes URI-like prefixes and "*" mirror names. Its only input here is
	// a generated path below a fixed root, and this allowlist excludes its special syntax.
	let mut components = path
		.strip_prefix(SCRATCH_ROOT)
		.map_err(|_| AppendError::InvalidScratchPath)?
		.components();
	let (Some(Component::Normal(directory)), Some(Component::Normal(filename)), None) =
		(components.next(), components.next(), components.next())
	else {
		return Err(AppendError::InvalidScratchPath);
	};
	let generated_suffix = directory
		.as_bytes()
		.strip_prefix(DIRECTORY_PREFIX.as_bytes())
		.filter(|suffix| !suffix.is_empty() && suffix.iter().all(u8::is_ascii_alphanumeric));
	if generated_suffix.is_none() || filename != SCRATCH_NAME {
		return Err(AppendError::InvalidScratchPath);
	}
	CString::new(path.as_os_str().as_bytes()).map_err(|_| AppendError::InvalidScratchPath)
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use super::native_path;

	#[test]
	fn native_path_excludes_stream_provider_and_mirror_syntax() {
		assert!(native_path(Path::new("/private/tmp/starbank-mpq-abcdef/candidate.mpq")).is_ok());
		for path in [
			"http://example.invalid/a.mpq",
			"/private/tmp/starbank-mpq-a/candidate.mpq*other.mpq",
			"/private/tmp/starbank-mpq-a/candidate.mpq*x:y",
			"/private/tmp-fake/starbank-mpq-a/candidate.mpq",
			"/private/tmp/starbank-mpq-a/embedded\0nul.mpq",
			"/private/tmp/starbank-mpq-a/../candidate.mpq",
		] {
			assert!(native_path(Path::new(path)).is_err(), "{path:?}");
		}
	}
}
