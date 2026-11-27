use std::{fs, io, path::Path};

#[cfg(not(windows))]
use cap_std::fs::OpenOptions;
use cap_std::fs::{Dir, File};

#[cfg(windows)]
pub(crate) fn archive(scope: &Dir, relative: &Path) -> io::Result<File> {
	starbank_paths::open_windows_file_for_read(scope, relative)
}

#[cfg(not(windows))]
pub(crate) fn archive(scope: &Dir, relative: &Path) -> io::Result<File> {
	let mut options = OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use cap_std::fs::OpenOptionsExt;
		options.custom_flags(read_flags());
	}
	scope.open_with(relative, &options)
}

pub(crate) fn cache(path: &Path) -> io::Result<fs::File> {
	let mut options = fs::OpenOptions::new();
	options.read(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.custom_flags(read_flags());
	}
	let file = options.open(path)?;
	if !file.metadata()?.is_file() {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"cache is not a regular file",
		));
	}
	Ok(file)
}

#[cfg(unix)]
fn read_flags() -> i32 {
	use rustix::fs::OFlags;

	// A replaced directory entry must not stall a worker on a named pipe.
	(OFlags::NONBLOCK | OFlags::NOFOLLOW).bits().cast_signed()
}
