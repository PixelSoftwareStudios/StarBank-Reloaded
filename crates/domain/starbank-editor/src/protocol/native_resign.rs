use starbank_core::Revision;
use starbank_signature::{SigningContext, resign};

use crate::EditorError;

use super::Engine;

impl Engine {
	/// Re-signs an exact active revision as an undoable edit.
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
		let active = self
			.active
			.as_mut()
			.filter(|active| active.id == document_id)
			.ok_or(EditorError::WrongSession)?;
		let session = &mut active.session;
		session.require_revision(expected)?;
		let signed =
			resign(&session.document, context, session.limits).map_err(EditorError::Signing)?;
		let revision = if signed.source() == session.document.source() {
			expected
		} else {
			session.commit(signed, session.ids.clone(), session.id_cursor)?
		};
		session.context = Some(context.clone());
		Ok(revision)
	}
}
