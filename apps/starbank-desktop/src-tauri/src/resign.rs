use serde::{Deserialize, Serialize};
use starbank_editor::MAX_CONTROL_BYTES;
use starbank_signature::{SignatureStatus, SigningContext};
use tauri::{Manager as _, WebviewWindow};

use crate::{
	commands::{CommandError, DesktopEngine, OpenedBank},
	save::{ActiveTarget, write_rejected},
	selection::destination_context,
};

#[derive(Deserialize)]
#[serde(
	rename_all = "snake_case",
	tag = "kind",
	content = "context",
	deny_unknown_fields
)]
pub(crate) enum ManualResign {
	Check(ManualContext),
	Apply(ManualContext),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManualContext {
	author: String,
	player: String,
	bank_name: String,
}

impl ManualContext {
	pub(crate) fn validate(self) -> Result<SigningContext, CommandError> {
		if self
			.author
			.len()
			.saturating_add(self.player.len())
			.saturating_add(self.bank_name.len())
			> MAX_CONTROL_BYTES
		{
			return Err(write_rejected("The signing context is too large."));
		}
		SigningContext::new(self.author, self.player, self.bank_name).map_err(write_rejected)
	}
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
pub(crate) enum ResignReply {
	ManualContextRequired,
	Checked(CheckedSignature),
	Signed(OpenedBank),
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckedSignature {
	Unsigned,
	Malformed,
	MissingContext,
	Unsupported,
	Valid,
	Invalid,
}

impl From<SignatureStatus> for CheckedSignature {
	fn from(status: SignatureStatus) -> Self {
		match status {
			SignatureStatus::Unsigned => Self::Unsigned,
			SignatureStatus::Malformed(_) => Self::Malformed,
			SignatureStatus::MissingContext => Self::MissingContext,
			SignatureStatus::Valid => Self::Valid,
			SignatureStatus::Invalid => Self::Invalid,
			_ => Self::Unsupported,
		}
	}
}

#[tauri::command]
pub(crate) async fn resign_active_bank(
	window: WebviewWindow,
	target: ActiveTarget,
	manual: Option<ManualResign>,
) -> Result<ResignReply, CommandError> {
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let mut session = state.engine.lock().map_err(|_| CommandError::Unavailable)?;
		let (selected, expected) = session.selected_for(&target)?;
		let path = selected.path.clone();
		let revision = match (destination_context(&path), manual) {
			(Some(context), None) => session
				.editor
				.resign_native_active(&target.document_id, expected, &context)
				.map_err(write_rejected)?,
			(None, None) => return Ok(ResignReply::ManualContextRequired),
			(Some(_), Some(_)) => {
				return Err(write_rejected(
					"This bank uses the signing context of its file path.",
				));
			}
			(None, Some(ManualResign::Check(context))) => {
				let status = session
					.editor
					.preview_native_active_signature(
						&target.document_id,
						expected,
						&context.validate()?,
					)
					.map_err(write_rejected)?;
				return Ok(ResignReply::Checked(status.into()));
			}
			(None, Some(ManualResign::Apply(context))) => session
				.editor
				.resign_native_active_for_manual_context(
					&target.document_id,
					expected,
					&context.validate()?,
				)
				.map_err(write_rejected)?,
		};
		session
			.snapshot(&target.request_id, target.document_id, revision, &path)
			.map(ResignReply::Signed)
			.map_err(|_| CommandError::InvalidPacket)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}
