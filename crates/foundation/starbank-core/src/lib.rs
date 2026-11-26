//! Shared primitives for the bank editor and its platform adapters.

#![forbid(unsafe_code)]

mod account_handle;
mod revision;

pub use account_handle::is_sc2_account_handle;
pub use revision::{ParseRevisionError, Revision};
