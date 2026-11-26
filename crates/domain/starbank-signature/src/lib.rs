//! Legacy StarCraft II bank signature computation and read-only verification.

#![forbid(unsafe_code)]

mod canonical;
mod context;
mod digest;
mod verification;

pub use canonical::{ComputeSignatureError, compute_signature};
pub use context::{ContextField, InvalidContext, SigningContext};
pub use digest::{ParseSignatureError, Signature};
pub use verification::{MalformedSignature, SignatureStatus, verify_signature};
