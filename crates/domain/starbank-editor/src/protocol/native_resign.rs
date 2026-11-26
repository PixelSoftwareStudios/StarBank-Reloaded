use starbank_core::Revision;
use starbank_signature::{SignatureStatus, SigningContext, resign, verify_signature};

use crate::{EditorError, Session};

use super::Engine;

impl Engine {
	/// Checks a stored signature against explicit context at an exact active revision.
	///
	/// This only reads the document. It does not change the context or checkpoints.
	///
	/// # Errors
	///
	/// Rejects a missing document or stale revision.
	pub fn preview_native_active_signature(
		&self,
		document_id: &str,
		expected: Revision,
		context: &SigningContext,
	) -> Result<SignatureStatus, EditorError> {
		let active = self
			.active
			.as_ref()
			.filter(|active| active.id == document_id)
			.ok_or(EditorError::WrongSession)?;
		active.session.require_revision(expected)?;
		Ok(verify_signature(&active.session.document, Some(context)))
	}

	/// Re-signs an exact active revision as an undoable edit using trusted path context.
	///
	/// The native host authorizes the signing context for the current bank. This
	/// prepares no disk write and leaves the opening and saved checkpoints intact.
	///
	/// # Errors
	///
	/// Rejects a missing document, a stale revision, or a bank that cannot be signed.
	pub fn resign_native_active(
		&mut self,
		document_id: &str,
		expected: Revision,
		context: &SigningContext,
	) -> Result<Revision, EditorError> {
		let session = self.native_resign_session(document_id)?;
		let revision = revise_signature(session, expected, context)?;
		session.context = Some(context.clone());
		Ok(revision)
	}

	/// Re-signs an exact active revision using context supplied for this edit.
	///
	/// This operation never adopts the supplied context as source provenance.
	/// It updates only the in-memory document and history when the signature changes.
	///
	/// # Errors
	///
	/// Rejects a missing document, a stale revision, or a bank that cannot be signed.
	pub fn resign_native_active_for_manual_context(
		&mut self,
		document_id: &str,
		expected: Revision,
		context: &SigningContext,
	) -> Result<Revision, EditorError> {
		revise_signature(self.native_resign_session(document_id)?, expected, context)
	}

	fn native_resign_session(&mut self, document_id: &str) -> Result<&mut Session, EditorError> {
		self.active
			.as_mut()
			.filter(|active| active.id == document_id)
			.map(|active| &mut active.session)
			.ok_or(EditorError::WrongSession)
	}
}

fn revise_signature(
	session: &mut Session,
	expected: Revision,
	context: &SigningContext,
) -> Result<Revision, EditorError> {
	session.require_revision(expected)?;
	let signed =
		resign(&session.document, context, session.limits).map_err(EditorError::Signing)?;
	if signed.source() == session.document.source() {
		return Ok(expected);
	}
	session.commit(signed, session.ids.clone(), session.id_cursor)
}
