//! Reads StarCraft II bank XML and writes targeted edits without reformatting the document.
#![forbid(unsafe_code)]

mod encoding;
mod error;
mod limits;
mod model;
mod parse;
mod preflight;
mod serialize;
mod signature;
mod structural;

pub use error::{Location, ParseError, ParseErrorKind, SerializeError};
pub use limits::{LimitKind, Limits};
pub use model::{
	BankDocument, Key, KeyId, OpaqueReason, Section, SectionId, StoredSignature, TypedAttribute,
	ValueId, ValueKind, ValueNode,
};
pub use parse::parse;
pub use serialize::{ValueEdit, serialize};
pub use signature::replace_signature;
pub use structural::{
	StructuralEdit, StructuralEditError, StructuralEditResult, StructuralIdMap, StructuralNodeId,
	apply_structural_edit,
};
