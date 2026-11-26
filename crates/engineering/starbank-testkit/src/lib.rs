//! Deterministic synthetic inputs and benchmark sampling.

#![forbid(unsafe_code)]

mod bank;
mod benchmark;

pub use bank::{BANK_WORKLOADS, synthetic_bank};
pub use benchmark::{measure, print_header};
