use std::{
	path::{Path, PathBuf},
	sync::{Mutex, MutexGuard},
};

use sc2bank::{BankDocument, Limits, parse};
use serde::Serialize;
use starbank_fs::{CommitIntent, CommitOutcome, Fingerprint};
use starbank_signature::{SigningContext, resign, verify_signature};
use tauri::{Manager as _, State, WebviewWindow};

use crate::{
	commands::{CommandError, DesktopEngine},
	export::can_export_bank,
	resign::{CheckedSignature, ManualContext},
	save::write_rejected,
	selection::{
		BANK_EXTENSION, BANK_FILTER, bank_name, destination_context, pick_bank, read_bank,
	},
};

const UNKNOWN_SOURCE: &str = "Choose the external bank again to continue.";
const CHANGED_SOURCE: &str = "The selected external bank changed. Choose it again.";

#[derive(Default)]
pub(crate) struct ExternalSignService(Mutex<SourceRegistry>);

#[derive(Default)]
struct SourceRegistry {
	last_id: u64,
	source: Option<SelectedSource>,
}

struct SelectedSource {
	id: String,
	owner: String,
	path: PathBuf,
	fingerprint: Fingerprint,
}

#[derive(Serialize)]
pub(crate) struct ExternalSigningSource {
	id: String,
	name: String,
	requires_context: bool,
	signature_status: CheckedSignature,
}

impl ExternalSignService {
	fn lock(&self) -> Result<MutexGuard<'_, SourceRegistry>, CommandError> {
		self.0.lock().map_err(|_| CommandError::Unavailable)
	}
}

impl SourceRegistry {
	fn select(
		&mut self,
		owner: &str,
		path: PathBuf,
	) -> Result<ExternalSigningSource, CommandError> {
		let limits = Limits::default();
		let (name, snapshot) = read_bank(&path, limits).map_err(CommandError::File)?;
		let document = parse(snapshot.bytes(), limits).map_err(write_rejected)?;
		let context = destination_context(&path);
		self.last_id = self
			.last_id
			.checked_add(1)
			.ok_or(CommandError::Unavailable)?;
		let id = self.last_id.to_string();
		self.source = Some(SelectedSource {
			id: id.clone(),
			owner: owner.to_owned(),
			path,
			fingerprint: snapshot.fingerprint().clone(),
		});
		Ok(ExternalSigningSource {
			id,
			name,
			requires_context: context.is_none(),
			signature_status: verify_signature(&document, context.as_ref()).into(),
		})
	}

	fn for_owner(&self, owner: &str, id: &str) -> Result<&SelectedSource, CommandError> {
		self.source
			.as_ref()
			.filter(|source| source.owner == owner && source.id == id)
			.ok_or_else(|| write_rejected(UNKNOWN_SOURCE))
	}

	fn discard(&mut self, owner: &str, id: &str) {
		if self.for_owner(owner, id).is_ok() {
			self.source = None;
		}
	}

	fn publish(
		&mut self,
		owner: &str,
		id: &str,
		destination: &Path,
		manual: Option<SigningContext>,
	) -> Result<String, CommandError> {
		let selected = self.for_owner(owner, id)?;
		let name = bank_name(destination).map_err(CommandError::File)?;
		let context = signed_copy_context(&selected.path, destination, manual)?;
		let document = selected.read()?;
		let limits = Limits::default();
		let signed = resign(&document, &context, limits).map_err(write_rejected)?;
		match starbank_fs::commit(
			destination,
			signed.source(),
			CommitIntent::CreateNew,
			limits.document_bytes,
		) {
			CommitOutcome::CommittedVerified(_) => {
				self.source = None;
				Ok(name)
			}
			CommitOutcome::RejectedUnchanged(error) => {
				Err(CommandError::WriteRejected(error.to_string()))
			}
			CommitOutcome::CommitUncertain(_) => {
				self.source = None;
				Err(CommandError::WriteUncertain)
			}
		}
	}
}

impl SelectedSource {
	fn read(&self) -> Result<BankDocument, CommandError> {
		let limits = Limits::default();
		let (_, current) =
			read_bank(&self.path, limits).map_err(|_| write_rejected(CHANGED_SOURCE))?;
		if current.fingerprint() != &self.fingerprint {
			return Err(write_rejected(CHANGED_SOURCE));
		}
		parse(current.bytes(), limits).map_err(write_rejected)
	}
}

fn signed_copy_context(
	source: &Path,
	destination: &Path,
	manual: Option<SigningContext>,
) -> Result<SigningContext, CommandError> {
	if let Some(inferred) = destination_context(destination) {
		if let Some(supplied) = manual
			&& supplied != inferred
		{
			return Err(write_rejected(
				"The supplied context differs from the context of the destination path.",
			));
		}
		return Ok(inferred);
	}
	manual
		.or_else(|| destination_context(source))
		.ok_or_else(|| write_rejected("Supply a signing context for the external bank."))
}

#[tauri::command]
pub(crate) async fn pick_external_bank_to_resign(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
) -> Result<Option<ExternalSigningSource>, CommandError> {
	if !can_export_bank() {
		return Err(CommandError::Unavailable);
	}
	let _dialog = engine.dialog()?;
	let Some(file) = pick_bank(&window).await else {
		return Ok(None);
	};
	let path = file.path().to_path_buf();
	let owner = window.label().to_owned();
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		app.state::<ExternalSignService>()
			.lock()?
			.select(&owner, path)
			.map(Some)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn check_external_bank_signature(
	window: WebviewWindow,
	id: String,
	context: ManualContext,
) -> Result<CheckedSignature, CommandError> {
	let owner = window.label().to_owned();
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<ExternalSignService>();
		let registry = state.lock()?;
		let selected = registry.for_owner(&owner, &id)?;
		let document = selected.read()?;
		let context = context.validate()?;
		Ok(verify_signature(&document, Some(&context)).into())
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

#[tauri::command]
pub(crate) async fn export_external_signed_bank(
	window: WebviewWindow,
	engine: State<'_, DesktopEngine>,
	state: State<'_, ExternalSignService>,
	id: String,
	context: Option<ManualContext>,
) -> Result<Option<String>, CommandError> {
	if !can_export_bank() {
		return Err(CommandError::Unavailable);
	}
	let _dialog = engine.dialog()?;
	let owner = window.label().to_owned();
	let manual = context.map(ManualContext::validate).transpose()?;
	let suggested = {
		let registry = state.lock()?;
		let source = registry.for_owner(&owner, &id)?;
		let stem = source.path.file_stem().ok_or(CommandError::InvalidPacket)?;
		format!("{} signed.{BANK_EXTENSION}", stem.to_string_lossy())
	};
	let Some(destination) = rfd::AsyncFileDialog::new()
		.set_parent(&window)
		.set_title("Save Signed Bank Copy")
		.set_file_name(&suggested)
		.add_filter(BANK_FILTER, &[BANK_EXTENSION])
		.save_file()
		.await
	else {
		return Ok(None);
	};
	let destination = destination.path().to_path_buf();
	let app = window.app_handle().clone();
	tauri::async_runtime::spawn_blocking(move || {
		app.state::<ExternalSignService>()
			.lock()?
			.publish(&owner, &id, &destination, manual)
			.map(Some)
	})
	.await
	.map_err(|_| CommandError::Unavailable)?
}

#[tauri::command]
pub(crate) fn discard_external_signing_source(
	window: WebviewWindow,
	state: State<'_, ExternalSignService>,
	id: String,
) -> Result<(), CommandError> {
	state.lock()?.discard(window.label(), &id);
	Ok(())
}

#[cfg(test)]
mod tests;
