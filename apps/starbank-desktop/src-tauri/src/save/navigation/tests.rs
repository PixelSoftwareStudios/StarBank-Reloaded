use std::{error::Error, fs, path::PathBuf};

use serde_json::{Value, json};

use super::*;

const ORIGINAL: &[u8] = b"<Bank><Section name='Original'/></Bank>";
const EDITED: &[u8] = b"<Bank><Section name='Edited'/></Bank>";

#[test]
fn dirty_engine_without_file_authority_offers_no_save_but_can_issue_scoped_discard()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, path) = fixture(true)?;
	session.selected = None;
	let before = session.editor.native_active_target().expect("dirty target");
	let probe =
		navigation_state(&session, &target(&session, "reopen"), true).map_err(native_error)?;
	assert!(probe.dirty);
	assert!(!probe.offer_save);
	assert!(matches!(
		finish_navigation(
			&mut session,
			active_target(&before, "reopen"),
			CloseChoice::Save,
			probe.offer_save
		),
		Err(CommandError::Unavailable)
	));
	assert!(session.discard.is_none());
	assert!(matches!(
		finish_navigation(
			&mut session,
			active_target(&before, "cancel"),
			CloseChoice::Cancel,
			false
		),
		Ok(CloseChoice::Cancel)
	));
	assert!(session.discard.is_none());
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("unchanged target"),
		before
	);
	assert!(matches!(
		finish_navigation(
			&mut session,
			active_target(&before, "reopen"),
			CloseChoice::Discard,
			false
		),
		Ok(CloseChoice::Discard)
	));
	let opened = session
		.open_path(&open_control("reopen", true), &path)
		.map_err(native_error)?;
	assert_eq!(reply_kind(&opened), "snapshot");
	assert!(session.discard.is_none());
	assert!(
		session
			.selected
			.as_ref()
			.expect("trusted reopening")
			.fingerprint
			.is_some()
	);
	assert!(
		!session
			.editor
			.native_active_target()
			.expect("reopened bank")
			.dirty
	);
	assert_eq!(fs::read(path)?, ORIGINAL);
	Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_selected_bank_without_an_overwrite_fingerprint_retains_normal_save_as_navigation()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, _) = fixture(true)?;
	session
		.selected
		.as_mut()
		.expect("selected bank")
		.fingerprint = None;
	let current = target(&session, "save-as");
	let probe = navigation_state(&session, &current, true).map_err(native_error)?;
	assert!(probe.dirty);
	assert!(
		probe.offer_save,
		"Save As remains authorized by the selected bank scope"
	);
	assert!(matches!(
		finish_navigation(&mut session, current, CloseChoice::Save, probe.offer_save),
		Ok(CloseChoice::Save)
	));
	assert!(session.discard.is_none());
	assert!(
		session
			.selected
			.as_ref()
			.expect("same selection")
			.fingerprint
			.is_none()
	);
	Ok(())
}

#[test]
fn a_clean_engine_without_file_authority_can_grant_the_explicit_reopen_permit()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, path) = fixture(false)?;
	session.selected = None;
	let current = target(&session, "clean-reopen");
	let probe = navigation_state(&session, &current, true).map_err(native_error)?;
	assert!(
		!probe.dirty,
		"the native command can take its no-prompt branch"
	);
	assert!(!probe.offer_save);
	assert!(matches!(
		finish_navigation(
			&mut session,
			current,
			CloseChoice::Discard,
			probe.offer_save
		),
		Ok(CloseChoice::Discard)
	));
	assert_eq!(
		reply_kind(
			&session
				.open_path(&open_control("clean-reopen", true), &path)
				.map_err(native_error)?
		),
		"snapshot"
	);
	assert!(session.selected.is_some());
	Ok(())
}

#[test]
fn navigation_rejects_invalid_request_bounds_and_stale_engine_scopes_without_a_permit()
-> Result<(), Box<dyn Error>> {
	let (_folder, session, _) = fixture(true)?;
	let before = session.editor.native_active_target().expect("target");
	for invalid in [
		ActiveTarget {
			request_id: String::new(),
			..target(&session, "unused")
		},
		ActiveTarget {
			request_id: "x".repeat(super::super::MAX_REQUEST_ID_BYTES + 1),
			..target(&session, "unused")
		},
		ActiveTarget {
			request_id: "line\nbreak".into(),
			..target(&session, "unused")
		},
		ActiveTarget {
			document_id: "another-document".into(),
			..target(&session, "valid")
		},
		ActiveTarget {
			revision: u64::MAX.to_string(),
			..target(&session, "valid")
		},
		ActiveTarget {
			revision: "bad-revision".into(),
			..target(&session, "valid")
		},
	] {
		assert!(navigation_state(&session, &invalid, false).is_err());
		assert!(session.discard.is_none());
		assert_eq!(
			session
				.editor
				.native_active_target()
				.expect("unchanged target"),
			before
		);
	}
	Ok(())
}

#[test]
fn a_revision_changed_while_the_native_prompt_was_open_cannot_authorize_discard()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, _) = fixture(true)?;
	let current = target(&session, "pending");
	let probe = navigation_state(&session, &current, true).map_err(native_error)?;
	assert!(probe.dirty);
	edit(&mut session, b"<Bank><Section name='Later'/></Bank>");
	let after = session.editor.native_active_target().expect("later target");
	assert!(
		finish_navigation(
			&mut session,
			current,
			CloseChoice::Discard,
			probe.offer_save
		)
		.is_err()
	);
	assert!(session.discard.is_none());
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("later state retained"),
		after
	);
	Ok(())
}

#[test]
fn request_mismatch_consumes_the_one_use_permit_and_does_not_discard_any_draft()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, path) = fixture(true)?;
	session.selected = None;
	let before = session.editor.native_active_target().expect("dirty target");
	finish_navigation(
		&mut session,
		active_target(&before, "approved"),
		CloseChoice::Discard,
		false,
	)
	.map_err(native_error)?;
	assert!(
		session
			.open_path(&open_control("other-request", true), &path)
			.is_err()
	);
	assert!(session.discard.is_none());
	assert!(
		session
			.open_path(&open_control("approved", true), &path)
			.is_err()
	);
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("draft remains"),
		before
	);
	assert_eq!(fs::read(path)?, ORIGINAL);
	Ok(())
}

#[test]
fn a_retired_document_scope_cannot_authorize_reopen_but_the_actual_engine_scope_can()
-> Result<(), Box<dyn Error>> {
	let (_folder, mut session, path) = fixture(false)?;
	let retired = target(&session, "retired");
	let new_open = session
		.editor
		.dispatch(&open_control("replacement", false), Some(ORIGINAL));
	let reply: Value = serde_json::from_str(&new_open.json)?;
	assert_eq!(reply["result"]["kind"], "snapshot");
	session.selected = None;
	assert!(navigation_state(&session, &retired, false).is_err());
	let current = target(&session, "current-reopen");
	assert_ne!(current.document_id, retired.document_id);
	let probe = navigation_state(&session, &current, false).map_err(native_error)?;
	assert!(!probe.dirty);
	finish_navigation(
		&mut session,
		current,
		CloseChoice::Discard,
		probe.offer_save,
	)
	.map_err(native_error)?;
	assert_eq!(
		reply_kind(
			&session
				.open_path(&open_control("current-reopen", true), &path)
				.map_err(native_error)?
		),
		"snapshot"
	);
	Ok(())
}

#[test]
fn an_engine_appearing_after_an_empty_recovery_scope_cannot_be_implicitly_replaced()
-> Result<(), Box<dyn Error>> {
	let folder = tempfile::tempdir()?;
	let path = folder.path().canonicalize()?.join("Current.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	let mut session = DesktopSession::default();
	assert!(session.editor.native_active_target().is_none());
	let opened = session
		.editor
		.dispatch(&open_control("appeared", false), Some(ORIGINAL));
	let reply: Value = serde_json::from_str(&opened.json)?;
	assert_eq!(reply["result"]["kind"], "snapshot");
	let before = session
		.editor
		.native_active_target()
		.expect("new clean engine");
	assert!(!before.dirty);
	assert!(session.selected.is_none());
	assert!(
		session
			.open_path(&open_control("unscoped", false), &path)
			.is_err()
	);
	assert_eq!(
		session
			.editor
			.native_active_target()
			.expect("new engine retained"),
		before
	);
	assert!(session.selected.is_none());
	assert_eq!(fs::read(path)?, ORIGINAL);
	Ok(())
}

fn fixture(dirty: bool) -> Result<(tempfile::TempDir, DesktopSession, PathBuf), Box<dyn Error>> {
	let folder = tempfile::tempdir()?;
	let path = folder.path().canonicalize()?.join("Current.SC2Bank");
	fs::write(&path, ORIGINAL)?;
	let mut session = DesktopSession::default();
	assert_eq!(
		reply_kind(
			&session
				.open_path(&open_control("initial", false), &path)
				.map_err(native_error)?
		),
		"snapshot"
	);
	if dirty {
		edit(&mut session, EDITED);
	}
	Ok((folder, session, path))
}

fn edit(session: &mut DesktopSession, bytes: &[u8]) {
	let active = session
		.editor
		.native_active_target()
		.expect("active target");
	let control = json!({
		"schema_version":starbank_editor::PROTOCOL_VERSION,
		"request_id":"edit",
		"document_id":active.document_id,
		"expected_revision":active.revision.to_string(),
		"command":{"kind":"replace_raw_text"}
	});
	let reply = session.dispatch(&control.to_string(), Some(bytes));
	let value: Value = serde_json::from_str(&reply.json).expect("wire control");
	assert_eq!(value["result"]["kind"], "snapshot");
}

fn target(session: &DesktopSession, request: &str) -> ActiveTarget {
	active_target(
		&session.editor.native_active_target().expect("engine scope"),
		request,
	)
}

fn active_target(active: &starbank_editor::NativeActiveTarget, request: &str) -> ActiveTarget {
	ActiveTarget {
		request_id: request.into(),
		document_id: active.document_id.clone(),
		revision: active.revision.to_string(),
	}
}

fn open_control(request: &str, discard: bool) -> String {
	json!({
		"schema_version":starbank_editor::PROTOCOL_VERSION,
		"request_id":request,
		"command":{"kind":"open","payload":{"discard_current":discard}}
	})
	.to_string()
}

fn reply_kind(opened: &crate::commands::OpenedBank) -> String {
	let value: Value = serde_json::from_str(&opened.control).expect("wire reply");
	value["result"]["kind"]
		.as_str()
		.expect("result kind")
		.into()
}

fn native_error(error: CommandError) -> std::io::Error {
	std::io::Error::other(format!("{error:?}"))
}
