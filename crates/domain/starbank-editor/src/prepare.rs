use std::sync::Arc;

use sc2bank::{BankDocument, parse, serialize};
use starbank_core::Revision;
use starbank_signature::{SignatureStatus, SigningContext, resign, verify_signature};

use crate::{EditorError, Session};

/// Signature policy for preparing exact output bytes.
#[derive(Clone, Copy, Debug)]
pub enum ExportSigning<'context> {
	/// Keep the stored signature and report its verification status.
	Preserve,
	/// Sign and verify against the intended destination context.
	Sign(&'context SigningContext),
}

/// Validated output and the session revision from which it was prepared.
#[derive(Debug)]
pub struct PreparedExport {
	bytes: Arc<[u8]>,
	revision: Revision,
	signature: SignatureStatus,
	token: Arc<()>,
	context: Option<SigningContext>,
	signed_document: Option<BankDocument>,
}

impl PreparedExport {
	/// The exact bytes to copy, export or commit.
	pub fn bytes(&self) -> &[u8] {
		&self.bytes
	}

	/// The revision captured during preparation.
	pub fn revision(&self) -> Revision {
		self.revision
	}

	/// Signature verification status for the prepared bytes and context.
	pub fn signature_status(&self) -> SignatureStatus {
		self.signature
	}
}

impl Session {
	/// Prepares checked output without changing the document or its checkpoints.
	///
	/// # Errors
	///
	/// Reports stale revisions, invalid output or signing failures.
	pub fn prepare(
		&self,
		expected: Revision,
		policy: ExportSigning<'_>,
	) -> Result<PreparedExport, EditorError> {
		self.require_revision(expected)?;
		let (bytes, signature, context, signed_document) = match policy {
			ExportSigning::Preserve => {
				let bytes =
					serialize(&self.document, &[], self.limits).map_err(EditorError::Serialize)?;
				let checked = parse(&bytes, self.limits).map_err(EditorError::Parse)?;
				let status = verify_signature(&checked, self.context.as_ref());
				(bytes.into(), status, self.context.clone(), None)
			}
			ExportSigning::Sign(context) => {
				let checked =
					resign(&self.document, context, self.limits).map_err(EditorError::Signing)?;
				(
					checked.source().into(),
					SignatureStatus::Valid,
					Some(context.clone()),
					Some(checked),
				)
			}
		};
		Ok(PreparedExport {
			bytes,
			revision: expected,
			signature,
			token: self.token.clone(),
			context,
			signed_document,
		})
	}

	/// Records a platform-verified save of the active document.
	///
	/// Consume the prepared operation only after the platform verifies the committed
	/// bytes. Later edits remain in memory and are compared against the new checkpoint.
	/// Copy and export-to-copy callers keep the operation outside this acknowledgement.
	///
	/// # Errors
	///
	/// Rejects a receipt from another session or a signed update that would exhaust revisions.
	pub fn acknowledge_verified_active_save(
		&mut self,
		mut prepared: PreparedExport,
	) -> Result<Revision, EditorError> {
		if !Arc::ptr_eq(&self.token, &prepared.token) {
			return Err(EditorError::WrongSession);
		}
		let revision = if prepared.revision == self.revision()
			&& let Some(document) = prepared.signed_document.take()
			&& document.source() != self.document.source()
		{
			self.commit(document, self.ids.clone(), self.id_cursor)?
		} else {
			self.revision()
		};
		self.context = prepared.context;
		self.saved_checkpoint(prepared.bytes);
		Ok(revision)
	}
}
