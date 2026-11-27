use std::path::Path;

use sc2bank::Limits;
use serde::Deserialize;
use serde_json::json;
use starbank_fs::FileSnapshot;

use crate::commands::{CommandError, OPEN_REQUEST, OpenedBank, is_request};
use crate::selection::{self, destination_context};

use super::{DesktopSession, SNAPSHOT, SelectedBank, reply_control};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenControl {
	schema_version: u32,
	request_id: String,
	command: OpenCommand,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCommand {
	#[serde(rename = "kind")]
	_kind: String,
	#[serde(default)]
	payload: OpenPayload,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct OpenPayload {
	#[serde(default)]
	discard_current: bool,
}

impl DesktopSession {
	pub(crate) fn open_path(
		&mut self,
		control: &str,
		path: &Path,
	) -> Result<OpenedBank, CommandError> {
		if !is_request(control, OPEN_REQUEST)? {
			return Err(CommandError::InvalidControl);
		}
		let request: OpenControl =
			serde_json::from_str(control).map_err(|_| CommandError::InvalidControl)?;
		let discard_current = self.discard.take().is_some_and(|permit| {
			request.command.payload.discard_current
				&& request.request_id == permit.request_id
				&& self.editor.native_active_target().is_some_and(|active| {
					active.document_id == permit.document_id && active.revision == permit.revision
				})
		});
		if self.selected.is_none()
			&& self.editor.native_active_target().is_some()
			&& !discard_current
		{
			return Err(CommandError::ActiveWriteRejected(
				"Confirm the current editor revision before opening another bank.".into(),
			));
		}
		let (name, observed) =
			selection::read_bank(path, Limits::default()).map_err(CommandError::File)?;
		self.open_observed(
			request.schema_version,
			&request.request_id,
			path,
			name,
			&observed,
			discard_current,
		)
	}

	pub(crate) fn open_observed(
		&mut self,
		schema_version: u32,
		request_id: &str,
		path: &Path,
		name: String,
		observed: &FileSnapshot,
		discard_current: bool,
	) -> Result<OpenedBank, CommandError> {
		let mut payload = json!({"discard_current":discard_current});
		if let Some(context) = destination_context(path) {
			payload["context"] = json!({
				"author": context.author(),
				"player": context.player(),
				"bank_name": context.bank_name()
			});
		}
		let control = json!({
			"schema_version": schema_version,
			"request_id": request_id,
			"command": {"kind": OPEN_REQUEST, "payload": payload}
		});
		let reply = self
			.editor
			.dispatch(&control.to_string(), Some(observed.bytes()));
		let parsed = reply_control(&reply)?;
		if parsed.result.kind == SNAPSHOT {
			let document_id = parsed.document_id.ok_or(CommandError::InvalidPacket)?;
			self.conflict.invalidate();
			self.backup_selection = None;
			self.backups = None;
			self.selected = Some(SelectedBank {
				document_id,
				path: path.to_path_buf(),
				fingerprint: Some(observed.fingerprint().clone()),
			});
		}
		Ok(OpenedBank {
			name,
			control: reply.json,
			breadcrumbs: selection::display_segments(path),
			can_overwrite: parsed.result.kind == SNAPSHOT
				&& starbank_fs::check_replace_scope(path).is_ok(),
		})
	}
}
