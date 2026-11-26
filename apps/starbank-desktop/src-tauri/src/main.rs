#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod commands;
mod desktop;
mod export;
mod frame;
mod selection;

fn main() -> tauri::Result<()> {
	desktop::run()
}
