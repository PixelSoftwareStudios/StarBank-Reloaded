//! Owns a bank editing session and prepares validated bytes for platform adapters.

#![forbid(unsafe_code)]

mod error;
mod history;
mod identity;
mod lexeme;
mod prepare;
mod protocol;
mod session;
mod structure;

pub use error::EditorError;
pub use identity::{OccurrenceId, ParseOccurrenceIdError};
pub use lexeme::{LexemeError, is_value_editable, normalize_value};
pub use prepare::{ExportSigning, PreparedExport};
pub use protocol::{
	Engine, MAX_CONTROL_BYTES, MAX_OPERATIONS, PROTOCOL_VERSION, WireReply, max_attachment_bytes,
};
pub use session::{Session, ValueChange};
pub use structure::{StructuralOutcome, StructureChange};
