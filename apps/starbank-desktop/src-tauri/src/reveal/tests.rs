use std::{fs, io, path::Path};

use serde_json::json;
use starbank_editor::PROTOCOL_VERSION;

use super::{ActiveTarget, DesktopSession, RevealIssue, validated_path};

const ORIGINAL: &[u8] = br#"<Bank version="1"><Section name="Initial"/></Bank>"#;
const EDITED: &[u8] = br#"<Bank version="1"><Section name="Edited"/></Bank>"#;

fn open(path: &Path) -> DesktopSession {
	let mut session = DesktopSession::default();
	let request = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "open",
		"command": {"kind": "open"}
	});
	session
		.open_path(&request.to_string(), path)
		.expect("open native fixture");
	session
}

fn target(session: &DesktopSession) -> ActiveTarget {
	let active = session
		.editor
		.native_active_target()
		.expect("active native fixture");
	ActiveTarget {
		request_id: "show-fixture".into(),
		document_id: active.document_id,
		revision: active.revision.to_string(),
	}
}

#[test]
fn reveal_reuses_only_the_selected_path_at_the_exact_revision() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let path = folder
		.path()
		.canonicalize()?
		.join("Bank with, punctuation.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	let mut session = open(&path);
	let previous = target(&session);
	assert_eq!(validated_path(&session, &previous), Ok(path.as_path()));

	let edit = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "edit",
		"document_id": previous.document_id,
		"expected_revision": previous.revision,
		"command": {"kind": "replace_raw"}
	});
	session.dispatch(&edit.to_string(), Some(EDITED));
	assert_eq!(
		validated_path(&session, &previous),
		Err(RevealIssue::TargetChanged)
	);

	let mut current = target(&session);
	assert_eq!(validated_path(&session, &current), Ok(path.as_path()));
	current.document_id = "wrong-document".into();
	assert_eq!(
		validated_path(&session, &current),
		Err(RevealIssue::TargetChanged)
	);
	current.revision = "not-a-revision".into();
	assert_eq!(
		validated_path(&session, &current),
		Err(RevealIssue::InvalidTarget)
	);
	Ok(())
}

#[test]
fn changed_or_disappeared_files_are_never_revealed_as_the_active_bank() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let path = folder.path().canonicalize()?.join("Current.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	let session = open(&path);
	let current = target(&session);
	fs::write(&path, EDITED)?;
	assert_eq!(
		validated_path(&session, &current),
		Err(RevealIssue::FileChanged)
	);
	fs::remove_file(&path)?;
	assert_eq!(
		validated_path(&session, &current),
		Err(RevealIssue::FileChanged)
	);
	Ok(())
}

#[test]
fn renderer_target_does_not_accept_a_file_path() {
	let forged = json!({
		"requestId": "requested-reveal",
		"documentId": "id",
		"revision": "0",
		"path": "caller-supplied.SC2Bank"
	});
	assert!(serde_json::from_value::<ActiveTarget>(forged).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn finder_receives_the_whole_path_as_one_argument() {
	use std::ffi::OsStr;

	let path = Path::new("/Users/local/Bank Cache/-R & bank.SC2Bank");
	let command = super::file_manager(path).expect("Finder command");
	assert_eq!(command.get_program(), OsStr::new("/usr/bin/open"));
	assert_eq!(
		command.get_args().collect::<Vec<_>>(),
		[OsStr::new("-R"), path.as_os_str()]
	);
}

#[cfg(windows)]
#[test]
fn explorer_receives_a_quoted_local_path_as_its_selection() {
	use std::ffi::OsStr;

	for path in [
		r"C:\Users\local\SC2, Banks\-open& bank.SC2Bank",
		r"\\?\C:\Users\local\SC2, Banks\-open& bank.SC2Bank",
	] {
		assert_eq!(
			super::explorer_select_arg(Path::new(path)).as_deref(),
			Some(OsStr::new(
				r#"/select,"C:\Users\local\SC2, Banks\-open& bank.SC2Bank""#
			)),
		);
	}
	for path in [
		r"\\server\bank.SC2Bank",
		r"C:bank.SC2Bank",
		r#"C:\user\"next argument".SC2Bank"#,
	] {
		assert_eq!(super::explorer_select_arg(Path::new(path)), None);
	}
}
