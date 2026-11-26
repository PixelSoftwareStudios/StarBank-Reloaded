//! Owns a bank editing session and prepares validated bytes for platform adapters.

#![forbid(unsafe_code)]

mod diff;
mod error;
mod history;
mod identity;
mod lexeme;
mod prepare;
mod profile;
mod protocol;
mod raw;
mod session;
mod structure;

pub use diff::{
	DiffOccurrence, DiffValue, OccurrenceChange, OccurrenceDiff, OccurrenceKind, OpeningDiff,
};
pub use error::EditorError;
pub use identity::{OccurrenceId, ParseOccurrenceIdError};
pub use lexeme::{LexemeError, is_value_editable, normalize_value};
pub use prepare::{ExportSigning, PreparedExport};
pub use profile::{ProfileEdit, ProfileEvidence, ProfileSessionError, ProfileValueChange};
pub use protocol::{
	Engine, MAX_CONTROL_BYTES, MAX_OPERATIONS, NativeActiveTarget, NativeRestoreError,
	NativeRestoreOutcome, NativeRestorePolicy, NativeRestoreReceipt, NativeSaveReceipt,
	PROTOCOL_VERSION, WireReply, max_attachment_bytes,
};
pub use raw::RawNumberKind;
pub use session::{Session, ValueChange};
pub use structure::{StructuralOutcome, StructureChange};
