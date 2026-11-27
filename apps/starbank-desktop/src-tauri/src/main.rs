#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod backup;
mod backup_location;
mod backup_retention;
mod backup_status;
mod batch_backup;
mod close_guard;
mod commands;
mod conflict;
mod desktop;
mod export;
mod external_sign;
mod frame;
mod inventory;
mod open_requests;
mod recovery;
mod resign;
mod reveal;
mod save;
mod selection;
mod settings;
mod xml_comparison;

fn main() -> tauri::Result<()> {
	desktop::run()
}
