use std::path::Path;

use sc2bank::{Limits, parse};
use serde::Serialize;
use serde_json::json;
use starbank_core::Revision;
use starbank_editor::{NativeActiveTarget, PROTOCOL_VERSION};
use starbank_fs::{FileSnapshot, Fingerprint, read_snapshot};
use tauri::{Manager as _, State, WebviewWindow};

use crate::commands::{CommandError, DesktopEngine, OpenedBank};
use crate::save::{ActiveTarget, DesktopSession, confirm, trusted_fingerprint};
use crate::selection::bank_name;

const BEFORE_DIFFERENCE_BYTES: usize = 2_048;
const EXCERPT_BYTES: usize = 16_384;
const RAW_TEXT: &str = "raw_text";

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct ConflictState {
	sequence: u64,
	review: Option<Observation>,
}

struct Observation {
	id: u64,
	document_id: String,
	revision: Revision,
	fingerprint: Fingerprint,
}

impl ConflictState {
	pub(crate) fn invalidate(&mut self) {
		self.review = None;
	}
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConflictReview {
	id: String,
	first_difference_line: Option<usize>,
	start_line: usize,
	editor: Excerpt,
	on_disk: Excerpt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Excerpt {
	text: String,
	omits_start: bool,
	omits_end: bool,
}

#[tauri::command]
pub(crate) async fn compare_bank_conflict(
	window: WebviewWindow,
	target: ActiveTarget,
) -> Result<ConflictReview, CommandError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		session.compare_conflict(&target)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn reopen_bank_conflict(
	window: WebviewWindow,
	state: State<'_, DesktopEngine>,
	target: ActiveTarget,
	review_id: String,
) -> Result<Option<OpenedBank>, CommandError> {
	let _dialog = state.dialog()?;
	let dirty = state
		.engine
		.lock()
		.map_err(|_| CommandError::Unavailable)?
		.review_target(&target, &review_id)?
		.dirty;

	if dirty
		&& !confirm(
			&window,
			"Reopen the bank from disk?",
			"Reopening the reviewed file will discard the unsaved changes in this editor.",
		)
		.await
	{
		return Ok(None);
	}

	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		session.reopen_review(&target, &review_id, dirty).map(Some)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

impl DesktopSession {
	fn compare_conflict(&mut self, target: &ActiveTarget) -> Result<ConflictReview, CommandError> {
		let (fresh, revision) = {
			let (selected, revision) = self.selected_for(target)?;
			let original = trusted_fingerprint(selected)?;
			let fresh = read_disk(&selected.path).and_then(|fresh| {
				if fresh.fingerprint() == original {
					return Err(cannot_compare(
						"The selected bank still matches the observed file.",
					));
				}
				Ok(fresh)
			});
			(fresh, revision)
		};
		self.conflict.invalidate();
		let fresh = fresh?;

		let disk = parse(fresh.bytes(), Limits::default()).map_err(cannot_compare)?;
		let request = json!({
			"schema_version": PROTOCOL_VERSION,
			"request_id": target.request_id,
			"document_id": target.document_id,
			"expected_revision": revision.to_string(),
			"command": {"kind": RAW_TEXT}
		});
		let current = self
			.editor
			.dispatch(&request.to_string(), None)
			.bytes
			.ok_or(CommandError::InvalidPacket)?;
		let current = std::str::from_utf8(&current).map_err(|_| CommandError::InvalidPacket)?;
		let id = self
			.conflict
			.sequence
			.checked_add(1)
			.ok_or(CommandError::Unavailable)?;
		let review = excerpt_review(id, current, disk.xml_text());

		self.conflict.sequence = id;
		self.conflict.review = Some(Observation {
			id,
			document_id: target.document_id.clone(),
			revision,
			fingerprint: fresh.fingerprint().clone(),
		});
		Ok(review)
	}

	fn review_target(
		&self,
		target: &ActiveTarget,
		review_id: &str,
	) -> Result<NativeActiveTarget, CommandError> {
		let (selected, revision) = self.selected_for(target)?;
		trusted_fingerprint(selected)?;
		let review_id: u64 = review_id
			.parse()
			.map_err(|_| CommandError::ConflictReviewChanged)?;
		if !self.conflict.review.as_ref().is_some_and(|review| {
			review.id == review_id
				&& review.document_id == target.document_id
				&& review.revision == revision
		}) {
			return Err(CommandError::ConflictReviewChanged);
		}
		self.editor
			.native_active_target()
			.ok_or(CommandError::Unavailable)
	}

	fn reopen_review(
		&mut self,
		target: &ActiveTarget,
		review_id: &str,
		discard_current: bool,
	) -> Result<OpenedBank, CommandError> {
		let active = self.review_target(target, review_id)?;
		if active.dirty && !discard_current {
			return Err(CommandError::ConflictReviewChanged);
		}

		let (selected, _) = self.selected_for(target)?;
		let path = selected.path.clone();
		let reviewed = self
			.conflict
			.review
			.take()
			.ok_or(CommandError::ConflictReviewChanged)?;
		let fresh = read_disk(&path).map_err(|_| CommandError::ConflictReviewChanged)?;
		if &reviewed.fingerprint != fresh.fingerprint() {
			return Err(CommandError::ConflictReviewChanged);
		}
		let name = bank_name(&path).map_err(CommandError::File)?;
		self.clear_discard();
		self.open_observed(
			PROTOCOL_VERSION,
			&target.request_id,
			&path,
			name,
			&fresh,
			discard_current,
		)
	}
}

fn read_disk(path: &Path) -> Result<FileSnapshot, CommandError> {
	read_snapshot(path, Limits::default().document_bytes).map_err(cannot_compare)
}

fn cannot_compare(reason: impl std::fmt::Display) -> CommandError {
	CommandError::CannotCompareBank(reason.to_string())
}

fn excerpt_review(id: u64, editor: &str, disk: &str) -> ConflictReview {
	let common_prefix = editor
		.bytes()
		.zip(disk.bytes())
		.take_while(|(editor, disk)| editor == disk)
		.count();
	let first_difference = editor.floor_char_boundary(common_prefix);
	let (first_difference_line, start) = if common_prefix == editor.len()
		&& common_prefix == disk.len()
	{
		(None, 0)
	} else {
		(
			Some(line_at(editor, first_difference)),
			editor.floor_char_boundary(first_difference.saturating_sub(BEFORE_DIFFERENCE_BYTES)),
		)
	};
	ConflictReview {
		id: id.to_string(),
		first_difference_line,
		start_line: line_at(editor, start),
		editor: excerpt(editor, start),
		on_disk: excerpt(disk, start),
	}
}

fn line_at(text: &str, index: usize) -> usize {
	let bytes = text.as_bytes();
	bytes[..index]
		.iter()
		.enumerate()
		.filter(|(offset, byte)| {
			**byte == b'\n' || (**byte == b'\r' && bytes.get(offset + 1) != Some(&b'\n'))
		})
		.count()
		+ 1
}

fn excerpt(text: &str, start: usize) -> Excerpt {
	let end = text.floor_char_boundary(text.len().min(start + EXCERPT_BYTES));
	Excerpt {
		text: text[start..end].to_owned(),
		omits_start: start > 0,
		omits_end: end < text.len(),
	}
}
