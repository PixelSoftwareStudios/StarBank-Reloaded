//! Shared primitives for the bank editor and its platform adapters.

#![forbid(unsafe_code)]

mod revision;

pub use revision::{ParseRevisionError, Revision};
