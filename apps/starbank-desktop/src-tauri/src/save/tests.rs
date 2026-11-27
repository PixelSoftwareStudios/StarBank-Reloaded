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

	for permit in [
		None,
		Some(DiscardPermit {
			request_id: "open".into(),
			document_id: opening.document_id,
			revision: opening.revision,
		}),
		Some(DiscardPermit {
			request_id: "older-open".into(),
			document_id: active.document_id.clone(),
			revision: active.revision,
		}),
	] {
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

	session.discard = Some(DiscardPermit {
		request_id: "open".into(),
		document_id: active.document_id,
		revision: active.revision,
	});
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

#[cfg(windows)]
#[test]
fn windows_save_as_only_acknowledges_a_verified_new_destination() -> io::Result<()> {
	const OCCUPANT: &[u8] = b"<Bank><Section name='Independent'/></Bank>";
	let folder = tempfile::tempdir()?;
	let root = folder.path().canonicalize()?;
	let source = root.join("Original.SC2Bank");
	let occupied = root.join("Occupied.SC2Bank");
	let destination = root.join("New.SC2Bank");
	fs::write(&source, ORIGINAL)?;
	fs::write(&occupied, OCCUPANT)?;
	let mut session = DesktopSession::default();
	let opened = session.open_path(OPEN, &source).expect("open source");
	assert!(!opened.can_overwrite);
	let opening = session
		.editor
		.native_active_target()
		.expect("opening scope");
	let source_snapshot =
		starbank_fs::read_snapshot(&source, max_attachment_bytes()).expect("read source");
	let edit = json!({
		"schema_version": PROTOCOL_VERSION,
		"request_id": "edit",
		"document_id": opening.document_id,
		"expected_revision": opening.revision.to_string(),
		"command": { "kind": "replace_raw" }
	});
	session.dispatch(&edit.to_string(), Some(EDITED));
	let dirty = session
		.editor
		.native_active_target()
		.expect("dirty current bank");
	assert!(dirty.dirty);
	let mut target = ActiveTarget {
		request_id: "save-as".into(),
		document_id: dirty.document_id.clone(),
		revision: opening.revision.to_string(),
	};
	let prepare = |session: &DesktopSession, path: &Path| PreparedSave {
		receipt: session
			.editor
			.prepare_native_active_save(
				&dirty.document_id,
				dirty.revision,
				starbank_editor::ExportSigning::PreserveFor(None),
			)
			.expect("prepare the current revision"),
		path: path.to_path_buf(),
		destination: Destination::Create(path.to_path_buf()),
		warn_signature: false,
		automatic_backup: AutomaticBackup::Off,
	};
	let stale = prepare(&session, &destination);
	assert!(matches!(
		publish_save_as(&mut session, &target, stale),
		Err(CommandError::ActiveWriteRejected(_))
	));
	assert!(!destination.exists());

	target.revision = dirty.revision.to_string();
	let collision = prepare(&session, &occupied);
	assert!(matches!(
		publish_save_as(&mut session, &target, collision),
		Err(CommandError::ActiveWriteRejected(_))
	));
	assert_eq!(fs::read(&occupied)?, OCCUPANT);
	assert_eq!(session.editor.native_active_target().as_ref(), Some(&dirty));
	let selected = session.selected.as_ref().expect("original authority");
	assert_eq!(selected.path, source);
	assert_eq!(
		trusted_fingerprint(selected).expect("trusted original"),
		source_snapshot.fingerprint()
	);
	assert_eq!(
		fs::read_dir(&root)?.count(),
		2,
		"no pending files after rejection"
	);

	let prepared = prepare(&session, &destination);
	let expected = prepared.receipt.bytes().to_vec();
	let opened = publish_save_as(&mut session, &target, prepared).expect("verified Save As");
	assert_eq!(reply_kind(&opened), SNAPSHOT);
	assert_eq!(opened.name, "New.SC2Bank");
	assert!(!opened.can_overwrite);
	let source_after =
		starbank_fs::read_snapshot(&source, max_attachment_bytes()).expect("read original");
	assert_eq!(source_after.fingerprint(), source_snapshot.fingerprint());
	assert_eq!(source_after.bytes(), ORIGINAL);
	assert_eq!(fs::read(&destination)?, expected);
	let saved = session
		.editor
		.native_active_target()
		.expect("active saved bank");
	assert!(!saved.dirty);
	assert_eq!(saved.document_id, dirty.document_id);
	let selected = session.selected.expect("new trusted selection");
	assert_eq!(selected.path, destination);
	let fresh = starbank_fs::read_snapshot(&destination, max_attachment_bytes())
		.expect("read verified destination");
	assert_eq!(
		trusted_fingerprint(&selected).expect("trusted new file"),
		fresh.fingerprint()
	);
	assert_eq!(fs::read_dir(root)?.count(), 3, "no pending or backup files");
	Ok(())
}

fn reply_kind(opened: &OpenedBank) -> String {
	serde_json::from_str::<ReplyControl>(&opened.control)
		.expect("engine reply")
		.result
		.kind
}
