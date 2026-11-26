//! Checks the layout of authored Rust source files in the checkout.

#![forbid(unsafe_code)]

mod layout;
mod repository;

use std::process::ExitCode;

fn main() -> ExitCode {
	repository::run()
}
