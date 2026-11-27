use std::{fs, io, path::PathBuf};

use super::*;
use crate::xml_comparison::EXCERPT_BYTES;

const ORIGINAL: &str = "<Bank><Section name='Wallet'/></Bank>";
const EDITED: &str = "<Bank><Section name='Edited'/></Bank>";
const EXTERNAL: &str = "<Bank><Section name='OnDisk'/></Bank>";
const NEXT: &str = "<Bank><Section name='Newer'/></Bank>";

struct Fixture {
	_directory: tempfile::TempDir,
	path: PathBuf,
	session: DesktopSession,
}

impl Fixture {
	fn new() -> io::Result<Self> {
		let directory = tempfile::tempdir()?;
		let path = directory.path().canonicalize()?.join("Fixture.SC2Bank");
		fs::write(&path, ORIGINAL)?;
		let mut session = DesktopSession::default();
		session
			.open_path(
				r#"{"schema_version":1,"request_id":"open","command":{"kind":"open"}}"#,
				&path,
			)
			.expect("open synthetic selected bank");
		Ok(Self {
			_directory: directory,
			path,
			session,
		})
	}

	fn target(&self) -> ActiveTarget {
		let active = self
			.session
			.editor
			.native_active_target()
			.expect("bank open");
		ActiveTarget {
			document_id: active.document_id,
			revision: active.revision.to_string(),
			request_id: "review".into(),
		}
	}

	fn editor_command(&mut self, kind: &str, bytes: Option<&[u8]>) -> starbank_editor::WireReply {
		let target = self.target();
		let control = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": "fixture",
			"document_id": target.document_id,
			"expected_revision": target.revision,
			"command": {"kind": kind}
		});
		self.session.dispatch(&control.to_string(), bytes)
	}

	fn editor_text(&mut self) -> String {
		String::from_utf8(
			self.editor_command("raw_text", None)
				.bytes
				.expect("editor XML attachment"),
		)
		.expect("editor UTF-8")
	}
}

#[test]
fn comparison_preserves_edits_and_reopen_requires_the_review_and_dirty_consent() -> io::Result<()> {
	let mut fixture = Fixture::new()?;
	fixture.editor_command("replace_raw_text", Some(EDITED.as_bytes()));
	fs::write(&fixture.path, EXTERNAL)?;
	let target = fixture.target();
	let before = fixture.session.editor.native_active_target();

	let review = fixture
		.session
		.compare_conflict(&target)
		.expect("changed bank");
	assert_eq!(review.editor.text, EDITED);
	assert_eq!(review.on_disk.text, EXTERNAL);
	assert_eq!(review.first_difference_line, Some(1));
	assert_eq!(fixture.session.editor.native_active_target(), before);
	assert_eq!(fs::read_to_string(&fixture.path)?, EXTERNAL);
	assert_eq!(fixture.editor_text(), EDITED);

	assert!(matches!(
		fixture.session.reopen_review(&target, &review.id, false),
		Err(CommandError::ConflictReviewChanged)
	));
	assert_eq!(fixture.editor_text(), EDITED);
	fixture
		.session
		.reopen_review(&target, &review.id, true)
		.expect("reopen exact reviewed file with discard consent");
	let reopened = fixture
		.session
		.editor
		.native_active_target()
		.expect("reopened bank");
	assert_ne!(reopened.document_id, target.document_id);
	assert!(!reopened.dirty);
	assert_eq!(fixture.editor_text(), EXTERNAL);
	assert_eq!(fs::read_to_string(&fixture.path)?, EXTERNAL);
	Ok(())
}

#[test]
fn comparison_expires_when_the_file_or_editor_changes_again() -> io::Result<()> {
	let mut fixture = Fixture::new()?;
	fs::write(&fixture.path, EXTERNAL)?;
	let target = fixture.target();
	let first = fixture
		.session
		.compare_conflict(&target)
		.expect("first review");
	fs::write(&fixture.path, NEXT)?;
	assert!(matches!(
		fixture.session.reopen_review(&target, &first.id, false),
		Err(CommandError::ConflictReviewChanged)
	));
	assert_eq!(fixture.editor_text(), ORIGINAL);
	assert_eq!(fs::read_to_string(&fixture.path)?, NEXT);

	let second = fixture
		.session
		.compare_conflict(&target)
		.expect("second review");
	assert_ne!(first.id, second.id);
	fixture.editor_command("replace_raw_text", Some(EDITED.as_bytes()));
	let current = fixture.target();
	assert!(matches!(
		fixture.session.reopen_review(&current, &second.id, true),
		Err(CommandError::ConflictReviewChanged)
	));
	assert_eq!(fixture.editor_text(), EDITED);
	assert_eq!(fs::read_to_string(&fixture.path)?, NEXT);
	Ok(())
}

#[test]
fn malformed_or_oversized_disk_data_cannot_replace_the_editor() -> io::Result<()> {
	let mut fixture = Fixture::new()?;
	fs::write(&fixture.path, EXTERNAL)?;
	let target = fixture.target();
	let previous = fixture
		.session
		.compare_conflict(&target)
		.expect("valid review");
	fs::write(&fixture.path, "<Bank>")?;
	assert!(matches!(
		fixture.session.compare_conflict(&target),
		Err(CommandError::CannotCompareBank(_))
	));
	assert!(matches!(
		fixture.session.reopen_review(&target, &previous.id, true),
		Err(CommandError::ConflictReviewChanged)
	));
	let file = fs::OpenOptions::new().write(true).open(&fixture.path)?;
	file.set_len(Limits::default().document_bytes as u64 + 1)?;
	assert!(matches!(
		fixture.session.compare_conflict(&target),
		Err(CommandError::CannotCompareBank(_))
	));
	assert_eq!(fixture.editor_text(), ORIGINAL);
	Ok(())
}

#[cfg(unix)]
#[test]
fn comparison_refuses_a_substituted_symlink() -> io::Result<()> {
	let mut fixture = Fixture::new()?;
	let current = fixture.target();
	let unauthorized = fixture.path.with_file_name("Other.SC2Bank");
	fs::write(&unauthorized, EXTERNAL)?;
	fs::remove_file(&fixture.path)?;
	std::os::unix::fs::symlink(unauthorized, &fixture.path)?;
	assert!(matches!(
		fixture.session.compare_conflict(&current),
		Err(CommandError::CannotCompareBank(_))
	));
	assert_eq!(fixture.editor_text(), ORIGINAL);
	Ok(())
}

#[test]
fn excerpts_include_late_unicode_difference_and_are_bounded() {
	let before = format!("First\r\nSecond\r{}", "Å".repeat(EXCERPT_BYTES));
	let left = format!("{before}😁{}", "é".repeat(EXCERPT_BYTES));
	let right = format!("{before}😂{}", "é".repeat(EXCERPT_BYTES));
	let review = excerpt_review(1, &left, &right);
	assert_eq!(review.start_line, 3);
	assert_eq!(review.first_difference_line, Some(3));
	assert!(review.editor.text.contains('😁'));
	assert!(review.on_disk.text.contains('😂'));
	for excerpt in [review.editor, review.on_disk] {
		assert!(excerpt.omits_start && excerpt.omits_end);
		assert!(excerpt.text.len() <= EXCERPT_BYTES);
	}
	assert_eq!(excerpt_review(2, &left, &left).first_difference_line, None);
}

#[test]
fn a_create_new_conflict_does_not_request_active_file_reopening() {
	use starbank_fs::{CommitOutcome, StorageError};

	let mut session = DesktopSession::default();
	for active in [false, true] {
		let blocked = crate::save::verify_write(
			&mut session,
			CommitOutcome::RejectedUnchanged(StorageError::Conflict),
			crate::recovery::WriteContext {
				operation: if active {
					crate::recovery::RecoveryOperation::SaveOverwrite
				} else {
					crate::recovery::RecoveryOperation::SaveAs
				},
				destination: std::path::Path::new("synthetic.SC2Bank"),
				backup: None,
			},
		);
		if active {
			assert!(matches!(blocked, Err(CommandError::ExternalBankChanged)));
		} else {
			assert!(matches!(blocked, Err(CommandError::ActiveWriteRejected(_))));
		}
	}
}
