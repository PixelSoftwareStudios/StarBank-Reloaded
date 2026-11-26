//! Reads StarCraft II bank XML and writes targeted value edits without reformatting the document.
#![forbid(unsafe_code)]

mod error;
mod limits;
mod model;
mod parse;
mod preflight;
mod serialize;

pub use error::{Location, ParseError, ParseErrorKind, SerializeError};
pub use limits::{LimitKind, Limits};
pub use model::{
	BankDocument, Key, KeyId, OpaqueReason, Section, SectionId, StoredSignature, TypedAttribute,
	ValueId, ValueKind, ValueNode,
};
pub use parse::parse;
pub use serialize::{ValueEdit, serialize};
