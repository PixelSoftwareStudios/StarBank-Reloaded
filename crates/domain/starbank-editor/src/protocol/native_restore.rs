use std::{error::Error, fmt, sync::Arc};

use starbank_core::Revision;
use starbank_signature::SignatureStatus;

use crate::{EditorError, Session};

use super::{Engine, OpenDocument};

/// Whether a native restore can discard an edited active document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRestorePolicy {
	/// Require a clean active document.
	RequireClean,
	/// The host obtained explicit confirmation to discard the current edits.
	DiscardChanges,
}

/// A rejected preparation or acknowledgement of a native restore.
#[derive(Debug)]
#[non_exhaustive]
pub enum NativeRestoreError {
	/// The active document has edits without permission to discard them.
	DirtyDocument,
	/// There is no remaining document identifier for the restored session.
	DocumentIdExhausted,
	/// The session or the supplied backup could not be validated.
	Editor(EditorError),
}

impl fmt::Display for NativeRestoreError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::DirtyDocument => formatter.write_str("the active document has unsaved edits"),
			Self::DocumentIdExhausted => formatter.write_str("document identifiers are exhausted"),
			Self::Editor(error) => error.fmt(formatter),
		}
	}
}

impl Error for NativeRestoreError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Editor(error) => Some(error),
			_ => None,
		}
	}
}

impl From<EditorError> for NativeRestoreError {
	fn from(error: EditorError) -> Self {
		Self::Editor(error)
	}
}

/// A validated backup and its intended active document, kept by the native host.
#[must_use]
pub struct NativeRestoreReceipt {
	document_id: String,
	revision: Revision,
	token: Arc<()>,
	policy: NativeRestorePolicy,
	candidate: Session,
}

impl NativeRestoreReceipt {
	/// Returns the currently open document that the receipt can replace.
	pub fn document_id(&self) -> &str {
		&self.document_id
	}

	/// Returns the active revision selected for replacement.
	pub fn revision(&self) -> Revision {
		self.revision
	}

	/// Returns the exact backup bytes to restore.
	pub fn bytes(&self) -> &[u8] {
		self.candidate.document().source()
	}

	/// Reports the backup's signature status using the active target's context.
	pub fn signature_status(&self) -> SignatureStatus {
		self.candidate.signature_status()
	}
}

impl fmt::Debug for NativeRestoreReceipt {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("NativeRestoreReceipt")
			.field("revision", &self.revision())
			.field("output_bytes", &self.bytes().len())
			.finish_non_exhaustive()
	}
}

/// The clean editor session created from a verified native restore.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRestoreOutcome {
	/// Identifier issued for the restored session.
	pub document_id: String,
	/// Initial revision of the restored session.
	pub revision: Revision,
}

impl Engine {
	/// Validates a backup as a new session without changing the active session.
	///
	/// The host authorizes the native target and verifies the backup catalog entry.
	/// The receipt retains a clean, parsed copy of the exact bytes to restore.
	///
	/// # Errors
	///
	/// Rejects a stale or missing document, dirty edits without confirmation,
	/// exhausted identifiers, and any backup the bank parser cannot open.
	pub fn prepare_native_active_restore(
		&self,
		document_id: &str,
		expected: Revision,
		backup_bytes: &[u8],
		policy: NativeRestorePolicy,
	) -> Result<NativeRestoreReceipt, NativeRestoreError> {
		let active = self.restore_target(document_id)?;
		validate_restore_source(active, expected, policy)?;
		let _ = self.next_restore_sequence()?;
		let candidate = Session::open(
			backup_bytes,
			active.session.limits,
			active.session.context.clone(),
		)?;
		Ok(NativeRestoreReceipt {
			document_id: active.id.clone(),
			revision: expected,
			token: active.session.token.clone(),
			policy,
			candidate,
		})
	}

	/// Revalidates a restore receipt immediately before writing the backup.
	///
	/// A host sharing this engine across tasks holds its exclusive guard through
	/// revalidation, the verified storage operation, and acknowledgement.
	///
	/// # Errors
	///
	/// Rejects a replaced session, a changed revision, an unapproved dirty session,
	/// or exhausted document identifiers.
	pub fn validate_native_active_restore(
		&self,
		receipt: &NativeRestoreReceipt,
	) -> Result<(), NativeRestoreError> {
		self.validate_restore_receipt(receipt).map(|_| ())
	}

	/// Opens the restored session after storage verifies the exact committed backup bytes.
	///
	/// On rejected or uncertain storage outcomes the host drops the receipt. A
	/// successful acknowledgement replaces the opening and saved checkpoints and
	/// retires the former document ID and undo history.
	///
	/// # Errors
	///
	/// Rejects a stale, replaced or unapproved dirty session, or exhausted identifiers.
	pub fn acknowledge_verified_native_active_restore(
		&mut self,
		receipt: NativeRestoreReceipt,
	) -> Result<NativeRestoreOutcome, NativeRestoreError> {
		let sequence = self.validate_restore_receipt(&receipt)?;
		let active = self.activate_document(sequence, receipt.candidate);
		Ok(NativeRestoreOutcome {
			document_id: active.id.clone(),
			revision: active.session.revision(),
		})
	}

	fn restore_target(&self, document_id: &str) -> Result<&OpenDocument, NativeRestoreError> {
		self.active
			.as_ref()
			.filter(|active| active.id == document_id)
			.ok_or(NativeRestoreError::Editor(EditorError::WrongSession))
	}

	fn next_restore_sequence(&self) -> Result<u64, NativeRestoreError> {
		self.document_sequence
			.checked_add(1)
			.ok_or(NativeRestoreError::DocumentIdExhausted)
	}

	fn validate_restore_receipt(
		&self,
		receipt: &NativeRestoreReceipt,
	) -> Result<u64, NativeRestoreError> {
		let active = self.restore_target(&receipt.document_id)?;
		if !Arc::ptr_eq(&active.session.token, &receipt.token) {
			return Err(EditorError::WrongSession.into());
		}
		validate_restore_source(active, receipt.revision, receipt.policy)?;
		self.next_restore_sequence()
	}
}

fn validate_restore_source(
	active: &OpenDocument,
	expected: Revision,
	policy: NativeRestorePolicy,
) -> Result<(), NativeRestoreError> {
	active.session.require_revision(expected)?;
	if policy == NativeRestorePolicy::RequireClean && active.session.is_dirty() {
		return Err(NativeRestoreError::DirtyDocument);
	}
	Ok(())
}
