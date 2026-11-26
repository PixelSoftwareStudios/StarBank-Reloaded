use std::{fs, io};

use super::*;

const OPEN: &str = r#"{"schema_version":1,"request_id":"open","command":{"kind":"open"}}"#;
const DISCARD: &str = r#"{"schema_version":1,"request_id":"open","command":{"kind":"open","payload":{"discard_current":true}}}"#;
const ORIGINAL: &[u8] = br#"<Bank version="1"><Section name="Initial"/></Bank>"#;
const EDITED: &[u8] = br#"<Bank version="1"><Section name="Changed"/></Bank>"#;

#[test]
fn native_open_pins_the_fingerprint_of_the_exact_opened_bytes() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let path = folder.path().canonicalize()?.join("Current.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	let mut session = DesktopSession::default();
	session.open_path(OPEN, &path).expect("trusted file opens");
	let selected = session.selected.as_ref().expect("native selection");
	let disk = starbank_fs::read_snapshot(&path, max_attachment_bytes()).expect("read original");
	assert_eq!(
		trusted_fingerprint(selected).expect("fingerprint"),
		disk.fingerprint()
	);

	fs::write(&path, EDITED)?;
	let changed = starbank_fs::read_snapshot(&path, max_attachment_bytes()).expect("read change");
	assert_ne!(
		trusted_fingerprint(selected).expect("fingerprint"),
		changed.fingerprint()
	);
	assert_eq!(selected.path, path);
	Ok(())
}

#[test]
fn dirty_open_requires_in_scope_one_use_native_consent() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let folder = folder.path().canonicalize()?;
	let original = folder.join("Current.SC2Bank");
	let replacement = folder.join("Next.SC2Bank");
	fs::write(&original, ORIGINAL)?;
	fs::write(&replacement, ORIGINAL)?;
	let mut session = DesktopSession::default();
	session
		.open_path(OPEN, &original)
		.expect("initial file opens");
	let opening = session.editor.native_active_target().expect("open target");
	let edit = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "edit",
		"document_id": opening.document_id,
		"expected_revision": opening.revision.to_string(),
		"command": { "kind": "replace_raw" }
	});
	session.dispatch(&edit.to_string(), Some(EDITED));
	let active = session
		.editor
		.native_active_target()
		.expect("edited target");
	assert!(active.dirty);

	for permit in [None, Some((opening.document_id, opening.revision))] {
		session.discard = permit;
		let open = session
			.open_path(DISCARD, &replacement)
			.expect("wire reply");
		assert_ne!(reply_kind(&open), SNAPSHOT);
		assert_eq!(
			session.selected.as_ref().expect("unchanged authority").path,
			original
		);
		assert!(session.discard.is_none());
	}

	session.discard = Some((active.document_id.clone(), active.revision));
	let open = session
		.open_path(DISCARD, &replacement)
		.expect("authorized replacement");
	assert_eq!(reply_kind(&open), SNAPSHOT);
	let active = session
		.editor
		.native_active_target()
		.expect("new active target");
	assert_eq!(
		session
			.selected
			.as_ref()
			.expect("replacement authority")
			.document_id,
		active.document_id
	);
	assert!(session.discard.is_none());
	assert!(!active.dirty);
	Ok(())
}

#[test]
fn a_failed_open_preserves_the_active_document_and_selected_file() -> io::Result<()> {
	let folder = tempfile::tempdir()?;
	let folder = folder.path().canonicalize()?;
	let path = folder.join("Current.SC2Bank");
	let other = folder.join("Invalid.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	fs::write(&other, b"not a bank")?;
	let mut session = DesktopSession::default();
	session.open_path(OPEN, &path).expect("initial file opens");
	let opening = session.editor.native_active_target();
	let open = session.open_path(OPEN, &other).expect("wire reply");
	assert_ne!(reply_kind(&open), SNAPSHOT);
	assert_eq!(session.editor.native_active_target(), opening);
	assert_eq!(
		session.selected.as_ref().expect("unchanged authority").path,
		path
	);
	Ok(())
}

fn reply_kind(opened: &OpenedBank) -> String {
	serde_json::from_str::<ReplyControl>(&opened.control)
		.expect("engine reply")
		.result
		.kind
}
