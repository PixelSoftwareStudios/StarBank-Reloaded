use std::fmt;

use sc2bank::Limits;
use starbank_core::Revision;
use starbank_signature::SigningContext;

use crate::{ExportSigning, OccurrenceId, OpeningDiff, Session, ValueChange};

mod diff;
mod error;
mod native_restore;
mod native_save;
mod reply;
mod request;
mod snapshot;
mod structure;

use self::diff::WireOccurrenceKind;
use self::error::ProtocolError;
use self::reply::{ReplyResult, Scope};
use self::request::{Action, ContextOptions, OpenOptions, Operation, Target, WireEdit};
use self::snapshot::{Snapshot, WireSignatureStatus};

pub use self::native_restore::{
	NativeRestoreError, NativeRestoreOutcome, NativeRestorePolicy, NativeRestoreReceipt,
};
pub use self::native_save::{NativeActiveTarget, NativeSaveReceipt};
pub use self::reply::WireReply;

/// Version of the command and response schema.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum UTF-8 bytes permitted in one JSON control message.
pub const MAX_CONTROL_BYTES: usize = 1024 * 1024;

/// Maximum value changes permitted in one transaction.
pub const MAX_OPERATIONS: usize = 4096;

/// Maximum bytes permitted in one bank attachment.
pub fn max_attachment_bytes() -> usize {
	Limits::default().document_bytes
}

/// One portable editor endpoint scoped to its owning adapter.
#[derive(Default)]
pub struct Engine {
	active: Option<OpenDocument>,
	document_sequence: u64,
}

struct OpenDocument {
	id: String,
	session: Session,
}

enum Output {
	Snapshot,
	Diff(OpeningDiff),
	Raw(Vec<u8>),
	Prepared {
		status: WireSignatureStatus,
		bytes: Vec<u8>,
	},
}

impl Engine {
	/// Decodes and applies one bounded command and encodes its response.
	///
	/// Bank files travel as separate byte attachments. Rejected input and stale
	/// requests leave the current document unchanged.
	pub fn dispatch(&mut self, control: &str, bytes: Option<&[u8]>) -> WireReply {
		if control.len() > MAX_CONTROL_BYTES {
			return WireReply::error(None, Scope::default(), ProtocolError::RequestTooLarge);
		}
		let request = match request::decode(control) {
			Ok(request) => request,
			Err((id, error)) => {
				return WireReply::error(id.as_deref(), Scope::default(), error);
			}
		};
		match request.action {
			Action::Open(options) => match self.open(options, bytes) {
				Ok(active) => active.snapshot(&request.id),
				Err(error) => WireReply::error(Some(&request.id), Scope::default(), error),
			},
			Action::InSession(target, operation) => {
				self.session_command(&request.id, target, operation, bytes)
			}
		}
	}

	fn open(
		&mut self,
		options: OpenOptions,
		bytes: Option<&[u8]>,
	) -> Result<&OpenDocument, ProtocolError> {
		if !options.discard_current
			&& self
				.active
				.as_ref()
				.is_some_and(|active| active.session.is_dirty())
		{
			return Err(ProtocolError::DirtyDocument);
		}
		let limits = Limits::default();
		let bytes = require_attachment(bytes, limits.document_bytes)?;
		let sequence = self
			.document_sequence
			.checked_add(1)
			.ok_or(ProtocolError::Internal)?;
		let context = interpret_context(options.context)?;
		let session = Session::open(bytes, limits, context)
			.map_err(|error| ProtocolError::editor(&error, ProtocolError::InvalidDocument))?;
		Ok(self.activate_document(sequence, session))
	}

	fn activate_document(&mut self, sequence: u64, session: Session) -> &OpenDocument {
		self.document_sequence = sequence;
		self.active.insert(OpenDocument {
			id: format!("document-{sequence}"),
			session,
		})
	}

	fn session_command(
		&mut self,
		request_id: &str,
		target: Target,
		operation: Operation,
		bytes: Option<&[u8]>,
	) -> WireReply {
		let Some(active) = self
			.active
			.as_mut()
			.filter(|active| active.id == target.document_id)
		else {
			return WireReply::error(
				Some(request_id),
				Scope::default(),
				ProtocolError::UnknownDocument,
			);
		};
		if active.session.revision() != target.expected {
			return active.error(request_id, ProtocolError::StaleRevision);
		}
		if let Operation::Close(options) = operation {
			if bytes.is_some() {
				return active.error(request_id, ProtocolError::UnexpectedAttachment);
			}
			if active.session.is_dirty() && !options.discard {
				return active.error(request_id, ProtocolError::DirtyDocument);
			}
			let reply = WireReply::encode(
				Some(request_id),
				active.scope(),
				ReplyResult::Closed {},
				None,
			);
			self.active = None;
			return reply;
		}
		match execute(&mut active.session, target.expected, operation, bytes) {
			Ok(Output::Snapshot) => active.snapshot(request_id),
			Ok(Output::Diff(diff)) => WireReply::encode(
				Some(request_id),
				active.scope(),
				ReplyResult::Diff((&diff).into()),
				None,
			),
			Ok(Output::Raw(bytes)) => WireReply::encode(
				Some(request_id),
				active.scope(),
				ReplyResult::Raw {},
				Some(bytes),
			),
			Ok(Output::Prepared { status, bytes }) => WireReply::encode(
				Some(request_id),
				active.scope(),
				ReplyResult::Prepared {
					signature_status: status,
				},
				Some(bytes),
			),
			Err(error) => active.error(request_id, error),
		}
	}
}

impl fmt::Debug for Engine {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("Engine")
			.field("has_document", &self.active.is_some())
			.finish_non_exhaustive()
	}
}

impl OpenDocument {
	fn scope(&self) -> Scope<'_> {
		Scope {
			document_id: Some(&self.id),
			revision: Some(self.session.revision()),
		}
	}

	fn snapshot(&self, request_id: &str) -> WireReply {
		match Snapshot::from_session(&self.session) {
			Ok(snapshot) => WireReply::encode(
				Some(request_id),
				self.scope(),
				ReplyResult::Snapshot(snapshot),
				None,
			),
			Err(error) => self.error(request_id, error),
		}
	}

	fn error(&self, request_id: &str, error: ProtocolError) -> WireReply {
		WireReply::error(Some(request_id), self.scope(), error)
	}
}

fn execute(
	session: &mut Session,
	expected: Revision,
	operation: Operation,
	bytes: Option<&[u8]>,
) -> Result<Output, ProtocolError> {
	if matches!(operation, Operation::ReplaceRaw) {
		let attached = require_attachment(bytes, max_attachment_bytes())?;
		let _ = session
			.replace_raw(expected, attached)
			.map_err(|error| ProtocolError::editor(&error, ProtocolError::InvalidDocument))?;
		return Ok(Output::Snapshot);
	}
	if bytes.is_some() {
		return Err(ProtocolError::UnexpectedAttachment);
	}
	let _ = match operation {
		Operation::Snapshot => return Ok(Output::Snapshot),
		Operation::Raw => return Ok(Output::Raw(session.document().source().to_vec())),
		Operation::SetValues(edits) => session.set_values(expected, &interpret_edits(&edits)?),
		Operation::Structure(edit) => session
			.apply_structure(expected, edit.interpret()?)
			.map(|result| result.revision),
		Operation::Revert(revert) => {
			let id = parse_occurrence(&revert.id)?;
			match revert.kind {
				WireOccurrenceKind::Section => session.revert_section(expected, id),
				WireOccurrenceKind::Key => session.revert_key(expected, id),
				WireOccurrenceKind::Value => session.revert_value(expected, id),
			}
		}
		Operation::Diff => {
			return session
				.opening_diff()
				.map(Output::Diff)
				.map_err(|error| ProtocolError::editor(&error, ProtocolError::Internal));
		}
		Operation::Undo => session.undo(expected),
		Operation::Redo => session.redo(expected),
		Operation::Reset => session.reset(expected),
		Operation::Prepare(options) => return prepare(session, expected, options),
		Operation::ReplaceRaw | Operation::Close(_) => return Err(ProtocolError::Internal),
	}
	.map_err(|error| ProtocolError::editor(&error, ProtocolError::InvalidEdit))?;
	Ok(Output::Snapshot)
}

fn prepare(
	session: &Session,
	expected: Revision,
	options: ContextOptions,
) -> Result<Output, ProtocolError> {
	let context = interpret_context(options)?;
	let signing = context
		.as_ref()
		.map_or(ExportSigning::Preserve, ExportSigning::Sign);
	let prepared = session
		.prepare(expected, signing)
		.map_err(|error| ProtocolError::editor(&error, ProtocolError::ExportFailed))?;
	Ok(Output::Prepared {
		status: prepared.signature_status().into(),
		bytes: prepared.bytes().to_vec(),
	})
}

fn interpret_edits(edits: &[WireEdit]) -> Result<Vec<ValueChange<'_>>, ProtocolError> {
	edits
		.iter()
		.map(|edit| {
			Ok(ValueChange {
				id: parse_occurrence(&edit.id)?,
				value: &edit.value,
			})
		})
		.collect()
}

fn parse_occurrence(id: &str) -> Result<OccurrenceId, ProtocolError> {
	id.parse().map_err(|_| ProtocolError::InvalidOccurrence)
}

fn interpret_context(options: ContextOptions) -> Result<Option<SigningContext>, ProtocolError> {
	options
		.context
		.map(|context| {
			SigningContext::new(context.author, context.player, context.bank_name)
				.map_err(|_| ProtocolError::InvalidContext)
		})
		.transpose()
}

fn require_attachment(bytes: Option<&[u8]>, limit: usize) -> Result<&[u8], ProtocolError> {
	let bytes = bytes.ok_or(ProtocolError::MissingAttachment)?;
	if bytes.len() > limit {
		return Err(ProtocolError::AttachmentTooLarge);
	}
	Ok(bytes)
}
