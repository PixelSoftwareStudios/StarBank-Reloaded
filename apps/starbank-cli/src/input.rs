use std::path::Path;

use starbank_fs::{FileSnapshot, read_snapshot};

use crate::failure::Failure;

pub(crate) fn read(path: &Path, limit: usize) -> Result<FileSnapshot, Failure> {
	Ok(read_snapshot(path, limit)?)
}
