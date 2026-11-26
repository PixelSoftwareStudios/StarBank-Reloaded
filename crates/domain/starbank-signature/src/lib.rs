//! Legacy StarCraft II bank signature computation and verification and export preparation.

#![forbid(unsafe_code)]

mod canonical;
mod context;
mod digest;
mod prepare;
mod verification;

pub use canonical::{ComputeSignatureError, compute_signature};
pub use context::{ContextField, InvalidContext, SigningContext};
pub use digest::{ParseSignatureError, Signature};
pub use prepare::{ResignError, resign};
pub use verification::{MalformedSignature, SignatureStatus, verify_signature};
