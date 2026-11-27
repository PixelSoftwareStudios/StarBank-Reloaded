use std::{
	collections::{BTreeMap, BTreeSet},
	fmt::Display,
	num::NonZeroU32,
	path::{MAIN_SEPARATOR, Path},
	sync::Arc,
};

use serde::{Deserialize, Serialize};

use super::{BatchBackupError, MAX_BATCH_BANKS};
use crate::recovery::{DisplayPath, bounded_reason};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchReviewTarget {
	pub(super) run_id: NonZeroU32,
	pub(super) row_index: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchBackupReview {
	run_id: NonZeroU32,
	row_index: usize,
	kind: ReviewKind,
	backup: DisplayPath,
	recovery: Option<DisplayPath>,
	reason: String,
	reason_truncated: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReviewKind {
	SnapshotCommitUncertain,
	MetadataRejected,
	MetadataCommitUncertain,
}

pub(super) type ReviewRows = BTreeMap<usize, ReviewEvidence>;
pub(super) type SharedDirectories = BTreeSet<Arc<str>>;

pub(super) struct CompletedReview {
	pub(super) run_id: NonZeroU32,
	pub(super) owner: String,
	pub(super) rows: ReviewRows,
}

impl CompletedReview {
	pub(super) fn read(
		&self,
		owner: &str,
		target: BatchReviewTarget,
	) -> Result<BatchBackupReview, BatchBackupError> {
		if self.owner != owner
			|| self.run_id != target.run_id
			|| target.row_index >= MAX_BATCH_BANKS
		{
			return Err(BatchBackupError::InvalidReview);
		}
		let evidence = self
			.rows
			.get(&target.row_index)
			.ok_or(BatchBackupError::InvalidReview)?;
		Ok(evidence.report(target))
	}
}

pub(super) struct ReviewEvidence {
	kind: ReviewKind,
	backup: RecordedPath,
	recovery: Option<RecordedPath>,
	reason: String,
	reason_truncated: bool,
}

impl ReviewEvidence {
	pub(super) fn new(
		kind: ReviewKind,
		backup: &Path,
		recovery: Option<&Path>,
		reason: &dyn Display,
		directories: &mut SharedDirectories,
	) -> Self {
		let (reason, reason_truncated) = bounded_reason(reason);
		Self {
			kind,
			backup: RecordedPath::new(backup, directories),
			recovery: recovery.map(|path| RecordedPath::new(path, directories)),
			reason,
			reason_truncated,
		}
	}

	fn report(&self, target: BatchReviewTarget) -> BatchBackupReview {
		BatchBackupReview {
			run_id: target.run_id,
			row_index: target.row_index,
			kind: self.kind,
			backup: self.backup.display(),
			recovery: self.recovery.as_ref().map(RecordedPath::display),
			reason: self.reason.clone(),
			reason_truncated: self.reason_truncated,
		}
	}
}

struct RecordedPath {
	directory: Option<Arc<str>>,
	tail: String,
	truncated: bool,
	lossy: bool,
}

impl RecordedPath {
	fn new(path: &Path, directories: &mut SharedDirectories) -> Self {
		let DisplayPath {
			display,
			truncated,
			lossy,
		} = DisplayPath::new(path);
		let Some(end) = display.rfind(MAIN_SEPARATOR) else {
			return Self {
				directory: None,
				tail: display,
				truncated,
				lossy,
			};
		};
		// Split the already-bounded display string so joining cannot alter its spelling.
		let (prefix, tail) = display.split_at(end + MAIN_SEPARATOR.len_utf8());
		let directory = match directories.get(prefix) {
			Some(shared) => Arc::clone(shared),
			None => {
				let shared: Arc<str> = Arc::from(prefix);
				directories.insert(Arc::clone(&shared));
				shared
			}
		};
		Self {
			directory: Some(directory),
			tail: tail.to_owned(),
			truncated,
			lossy,
		}
	}

	fn display(&self) -> DisplayPath {
		let directory = self.directory.as_deref().unwrap_or_default();
		DisplayPath {
			display: [directory, self.tail.as_str()].concat(),
			truncated: self.truncated,
			lossy: self.lossy,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sibling_paths_share_directories_without_changing_display_spelling() {
		let backup = format!("folder{MAIN_SEPARATOR}Copy.SC2Bank.bak");
		let recovery = format!("folder{MAIN_SEPARATOR}.starbank-pending");
		let mut directories = SharedDirectories::new();
		let backup_path = RecordedPath::new(Path::new(&backup), &mut directories);
		let recovery_path = RecordedPath::new(Path::new(&recovery), &mut directories);
		assert_eq!(directories.len(), 1);
		assert!(Arc::ptr_eq(
			backup_path.directory.as_ref().expect("backup directory"),
			recovery_path
				.directory
				.as_ref()
				.expect("recovery directory")
		));
		assert_eq!(backup_path.display().display, backup);
		assert_eq!(recovery_path.display().display, recovery);
	}

	#[cfg(unix)]
	#[test]
	fn reassembled_path_preserves_truncation_and_lossy_flags() {
		use std::{ffi::OsString, os::unix::ffi::OsStringExt as _, path::PathBuf};

		let mut bytes = b"/fixture/\xff/".to_vec();
		bytes.extend("星".repeat(4096).bytes());
		let path = PathBuf::from(OsString::from_vec(bytes));
		let expected = DisplayPath::new(&path);
		let mut directories = SharedDirectories::new();
		let actual = RecordedPath::new(&path, &mut directories).display();
		assert_eq!(actual.display, expected.display);
		assert!(actual.truncated);
		assert!(actual.lossy);

		let reason = "✨".repeat(512);
		let detail = ReviewEvidence::new(
			ReviewKind::MetadataCommitUncertain,
			&path,
			Some(&path),
			&reason,
			&mut directories,
		);
		assert_eq!(detail.reason, bounded_reason(&reason).0);
		assert!(detail.reason_truncated);
	}
}
