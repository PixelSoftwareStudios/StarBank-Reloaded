//! Available file and directory flush through the qualified writable handle.

use crate::StorageError;
use std::fs::File;

pub(super) fn flush(file: &File, operation: &'static str) -> Result<(), StorageError> {
	// Rust's Windows File::sync_all calls FlushFileBuffers. The transaction requires a
	// writable qualified file/directory handle; access or flush failure is never optional.
	// This does not establish survival of an OS crash or a power interruption.
	file.sync_all()
		.map_err(|error| StorageError::io(operation, error))
}
