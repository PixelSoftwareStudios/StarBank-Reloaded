#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod backup;
mod batch_backup;
mod commands;
mod desktop;
mod export;
mod frame;
mod inventory;
mod open_requests;
mod resign;
mod reveal;
mod save;
mod selection;
mod settings;

fn main() -> tauri::Result<()> {
	desktop::run()
}
