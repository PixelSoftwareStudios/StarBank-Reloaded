//! Headless entry point for StarBank Reloaded.

#![forbid(unsafe_code)]

mod app;
mod args;
mod backup;
mod backup_command;
mod backup_metadata;
mod bank;
mod diff;
mod discovery;
mod failure;
mod input;
mod output;
mod patch;
mod write_bank;

fn main() -> std::process::ExitCode {
	app::run()
}
