//! WebAssembly binding for a single local editor engine.

#![forbid(unsafe_code)]

mod engine;

pub use engine::{WasmBankEngine, WasmReply};
