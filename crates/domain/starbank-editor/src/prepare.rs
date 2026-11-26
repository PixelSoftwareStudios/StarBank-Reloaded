use std::sync::{
	Arc,
	atomic::{AtomicU64, Ordering},
};

use sc2bank::{BankDocument, parse, serialize};
use starbank_core::Revision;
use starbank_signature::{SignatureStatus, SigningContext, resign, verify_signature};

use crate::{EditorError, Session};

/// Signature policy for preparing exact output bytes.
#[derive(Clone, Copy, Debug)]
pub enum ExportSigning<'context> {
	/// Keep the stored signature and check it against the active source context.
	Preserve,
	/// Keep the stored signature and check it against the intended destination.
	///
	/// A missing destination context deliberately leaves the signature unverified.
	PreserveFor(Option<&'context SigningContext>),
	/// Sign and verify against the intended destination context.
	Sign(&'context SigningContext),
}

/// Validated output and the session revision from which it was prepared.
#[derive(Debug)]
pub struct PreparedExport {
	bytes: Arc<[u8]>,
	revision: Revision,
	sequence: u64,
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

#[derive(Debug, Default)]
pub(crate) struct SaveOrder {
	next: AtomicU64,
	acknowledged: Option<u64>,
}

impl SaveOrder {
	fn issue(&self) -> Result<u64, EditorError> {
		// This counter assigns order only. The session's exclusive borrow controls acknowledgement.
		self.next
			.try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
				next.checked_add(1)
			})
			.map_err(|_| EditorError::PrepareOrderExhausted)
	}
}

impl Session {
	/// Prepares checked output without changing the document or its checkpoints.
	///
	/// # Errors
	///
	/// Reports stale revisions, invalid output, signing failures or exhausted preparation order.
	pub fn prepare(
		&self,
		expected: Revision,
		policy: ExportSigning<'_>,
	) -> Result<PreparedExport, EditorError> {
		self.require_revision(expected)?;
		let context = match policy {
			ExportSigning::Preserve => self.context.as_ref(),
			ExportSigning::PreserveFor(context) => context,
			ExportSigning::Sign(context) => Some(context),
		};
		let (bytes, signature, signed_document) = match policy {
			ExportSigning::Preserve | ExportSigning::PreserveFor(_) => {
				let bytes =
					serialize(&self.document, &[], self.limits).map_err(EditorError::Serialize)?;
				let checked = parse(&bytes, self.limits).map_err(EditorError::Parse)?;
				let status = verify_signature(&checked, context);
				(bytes.into(), status, None)
			}
			ExportSigning::Sign(context) => {
				let checked =
					resign(&self.document, context, self.limits).map_err(EditorError::Signing)?;
				(
					checked.source().into(),
					SignatureStatus::Valid,
					Some(checked),
				)
			}
		};
		Ok(PreparedExport {
			bytes,
			revision: expected,
			sequence: self.save_order.issue()?,
			signature,
			token: self.token.clone(),
			context: context.cloned(),
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
	/// Rejects a receipt from another session, a receipt superseded by a newer verified save,
	/// or a signed update that would exhaust revisions.
	pub fn acknowledge_verified_active_save(
		&mut self,
		mut prepared: PreparedExport,
	) -> Result<Revision, EditorError> {
		self.require_unacknowledged_active_save(&prepared)?;
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
		self.save_order.acknowledged = Some(prepared.sequence);
		Ok(revision)
	}

	pub(crate) fn validate_current_active_save(
		&self,
		prepared: &PreparedExport,
	) -> Result<(), EditorError> {
		self.require_unacknowledged_active_save(prepared)?;
		self.require_revision(prepared.revision)?;
		if prepared
			.signed_document
			.as_ref()
			.is_some_and(|document| document.source() != self.document.source())
		{
			let _ = self.next_revision()?;
		}
		Ok(())
	}

	fn require_unacknowledged_active_save(
		&self,
		prepared: &PreparedExport,
	) -> Result<(), EditorError> {
		if !Arc::ptr_eq(&self.token, &prepared.token) {
			return Err(EditorError::WrongSession);
		}
		if self
			.save_order
			.acknowledged
			.is_some_and(|sequence| prepared.sequence <= sequence)
		{
			return Err(EditorError::StaleSaveAcknowledgement);
		}
		Ok(())
	}
}
