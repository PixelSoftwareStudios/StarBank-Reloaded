//! Owns a bank editing session and prepares validated bytes for platform adapters.

#![forbid(unsafe_code)]

mod error;
mod history;
mod identity;
mod lexeme;
mod prepare;
mod session;

pub use error::EditorError;
pub use identity::{OccurrenceId, ParseOccurrenceIdError};
pub use lexeme::{LexemeError, is_value_editable, normalize_value};
pub use prepare::{ExportSigning, PreparedExport};
pub use session::{Session, ValueChange};
