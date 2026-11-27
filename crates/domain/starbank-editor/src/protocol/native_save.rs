use std::fmt;

use starbank_core::Revision;
use starbank_signature::SignatureStatus;

use crate::{EditorError, ExportSigning, PreparedExport};

use super::Engine;

/// A single-use receipt binding exact prepared bytes to an active editor session.
///
/// The native host keeps the receipt private and acknowledges it after a verified
/// commit to the active file. Serialized editor commands never carry a receipt.
#[must_use]
pub struct NativeSaveReceipt {
	document_id: String,
	prepared: PreparedExport,
}

impl NativeSaveReceipt {
	/// Identifies the document whose active target is authorized by the host.
	pub fn document_id(&self) -> &str {
		&self.document_id
	}

	/// Returns the revision used to prepare the output.
	pub fn revision(&self) -> Revision {
		self.prepared.revision()
	}

	/// Returns the exact output bytes for the native commit.
	pub fn bytes(&self) -> &[u8] {
		self.prepared.bytes()
	}

	/// Reports signature verification for the prepared output.
	pub fn signature_status(&self) -> SignatureStatus {
		self.prepared.signature_status()
	}
}

impl fmt::Debug for NativeSaveReceipt {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("NativeSaveReceipt")
			.field("revision", &self.revision())
			.field("output_bytes", &self.bytes().len())
			.finish_non_exhaustive()
	}
}

/// Current editor target for a native prompt or file operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeActiveTarget {
	/// The active session identifier.
	pub document_id: String,
	/// The current revision that the prompt or operation must verify.
	pub revision: Revision,
	/// Whether opening another file would discard edits.
	pub dirty: bool,
}

impl Engine {
	/// Reports whether the native renderer could still hold a projection or draft.
	///
	/// The history survives closing a bank. Rejected opens do not create history.
	pub fn native_has_opened_document(&self) -> bool {
		self.document_sequence != 0
	}

	/// Captures the active editor identity, revision, and dirty state for a native operation.
	pub fn native_active_target(&self) -> Option<NativeActiveTarget> {
		self.active.as_ref().map(|active| NativeActiveTarget {
			document_id: active.id.clone(),
			revision: active.session.revision(),
			dirty: active.session.is_dirty(),
		})
	}

	/// Prepares exact output for a host-authorized overwrite of the active file.
	///
	/// The native host owns destination authorization and fingerprint checks.
	///
	/// # Errors
	///
	/// Rejects a missing or replaced session, a stale revision, and invalid output.
	pub fn prepare_native_active_save(
		&self,
		document_id: &str,
		expected: Revision,
		signing: ExportSigning<'_>,
	) -> Result<NativeSaveReceipt, EditorError> {
		let active = self
			.active
			.as_ref()
			.filter(|active| active.id == document_id)
			.ok_or(EditorError::WrongSession)?;
		let prepared = active.session.prepare(expected, signing)?;
		active.session.validate_current_active_save(&prepared)?;
		Ok(NativeSaveReceipt {
			document_id: active.id.clone(),
			prepared,
		})
	}

	/// Revalidates the receipt immediately before a native overwrite.
	///
	/// A host sharing this engine across tasks must hold its exclusive guard
	/// through revalidation, the checked write, and acknowledgement.
	///
	/// # Errors
	///
	/// Rejects a replaced session, a changed revision, or a superseded receipt.
	pub fn validate_native_active_save(
		&self,
		receipt: &NativeSaveReceipt,
	) -> Result<(), EditorError> {
		self.active
			.as_ref()
			.filter(|active| active.id == receipt.document_id)
			.ok_or(EditorError::WrongSession)?
			.session
			.validate_current_active_save(&receipt.prepared)
	}

	/// Records a verified commit of the receipt's exact bytes to the active file.
	///
	/// The host calls this only after the storage adapter verifies its write.
	/// An unsuccessful or uncertain write discards the receipt. An acknowledgement
	/// of an earlier revision records its checkpoint while preserving later edits.
	///
	/// # Errors
	///
	/// Rejects a replaced session, a superseded receipt, or revision exhaustion.
	pub fn acknowledge_verified_native_active_save(
		&mut self,
		receipt: NativeSaveReceipt,
	) -> Result<Revision, EditorError> {
		self.active
			.as_mut()
			.filter(|active| active.id == receipt.document_id)
			.ok_or(EditorError::WrongSession)?
			.session
			.acknowledge_verified_active_save(receipt.prepared)
	}
}
