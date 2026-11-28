use tauri::{
	DragDropEvent, Manager as _, Url, WebviewEvent, WebviewWindowBuilder,
	webview::NewWindowResponse,
};

use crate::{
	batch_backup::BatchBackupService,
	close_guard::{self, DesktopCloseGuard},
	commands::DesktopEngine,
	inventory::InventoryService,
	open_requests,
	settings::SettingsStore,
};

#[cfg(windows)]
use crate::windows_launch;

const MAIN_WINDOW: &str = "main";
const LOCAL_WEBVIEW_HOST: &str = "localhost";
const WINDOWS_WEBVIEW_HOST: &str = "tauri.localhost";
const MAP_CACHE: &str = "map-index.json";
const INVENTORY_ROOT: &str = "inventory-root.json";

pub(crate) fn run() -> tauri::Result<()> {
	let context = tauri::generate_context!();
	#[cfg(windows)]
	let Some((requests, launch)) = windows_launch::begin(&context.config().identifier) else {
		return Ok(());
	};
	#[cfg(not(windows))]
	let requests = open_requests::OpenRequests::from_args(
		std::env::current_dir().ok().as_deref(),
		std::env::args_os().skip(1),
	);
	let builder = tauri::Builder::default();
	#[cfg(target_os = "macos")]
	let builder = builder.menu(close_guard::menu);
	builder
		.manage(DesktopEngine::default())
		.manage(DesktopCloseGuard::default())
		.manage(BatchBackupService::default())
		.manage(crate::external_sign::ExternalSignService::default())
		.manage(requests)
		.setup(move |app| {
			let cache_path = app.path().app_cache_dir().ok().and_then(|directory| {
				std::fs::create_dir_all(&directory).ok()?;
				std::fs::symlink_metadata(&directory)
					.ok()?
					.is_dir()
					.then(|| directory.join(MAP_CACHE))
			});
			let migrated_root = SettingsStore::for_app(app.handle())
				.and_then(|store| store.legacy_root_override())
				.ok()
				.flatten();
			let root_record = app
				.path()
				.app_config_dir()
				.ok()
				.map(|directory| directory.join(INVENTORY_ROOT));
			app.manage(
				InventoryService::with_cache_path(cache_path)
					.with_starting_directory(migrated_root)
					.with_root_record(root_record),
			);
			let config = app
				.config()
				.app
				.windows
				.iter()
				.find(|window| window.label == MAIN_WINDOW)
				.ok_or("missing main webview configuration")?;
			let development_origin = app.config().build.dev_url.clone();
			#[cfg(windows)]
			windows_launch::start(app.handle(), launch, MAIN_WINDOW);
			WebviewWindowBuilder::from_config(app, config)?
				.on_navigation(move |url| is_local_webview(url, development_origin.as_ref()))
				.on_new_window(|_, _| NewWindowResponse::Deny)
				.build()?;
			Ok(())
		})
		.on_webview_event(|webview, event| {
			if webview.label() == MAIN_WINDOW
				&& let WebviewEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event
			{
				open_requests::enqueue(webview.app_handle(), paths.iter().cloned());
			}
		})
		.invoke_handler(tauri::generate_handler![
			close_guard::confirm_window_close,
			close_guard::complete_window_close,
			crate::commands::open_bank,
			crate::commands::dispatch_bank,
			crate::clipboard::copy_bank_xml,
			crate::export::export_bank_copy,
			crate::export::can_export_bank,
			crate::export::can_create_bank_copy,
			open_requests::list_open_requests,
			open_requests::dismiss_open_request,
			open_requests::open_requested_bank,
			crate::resign::resign_active_bank,
			crate::external_sign::pick_external_bank_to_resign,
			crate::external_sign::check_external_bank_signature,
			crate::external_sign::export_external_signed_bank,
			crate::external_sign::discard_external_signing_source,
			crate::reveal::can_reveal_bank,
			crate::reveal::reveal_active_bank,
			crate::save::navigation::confirm_bank_navigation,
			crate::save::save_active_bank,
			crate::save::save_bank_as,
			crate::conflict::compare_bank_conflict,
			crate::conflict::reopen_bank_conflict,
			crate::backup::list_active_backups,
			crate::backup_status::active_backup_status,
			crate::backup::restore::review_active_backup,
			crate::backup::restore::restore_active_bank,
			crate::backup::export_copy::export_active_backup,
			crate::backup_location::read_backup_preferences,
			crate::backup_location::choose_backup_location,
			crate::backup_location::use_default_backup_location,
			crate::backup_location::set_backup_retention,
			crate::backup_retention::review_backup_retention,
			crate::batch_backup::create_batch_backup,
			crate::batch_backup::cancel_batch_backup,
			crate::batch_backup::review_batch_backup,
			crate::inventory::refresh_inventory,
			crate::inventory::inventory_backup_status,
			crate::inventory::rebuild_map_index,
			crate::inventory::select_inventory_folder,
			crate::inventory::use_automatic_inventory,
			crate::inventory::cancel_inventory,
			crate::inventory::open_bank_id,
			crate::settings::read_settings,
			crate::settings::change_setting,
			crate::settings::recover_settings
		])
		.build(context)?
		.run(on_run_event);
	Ok(())
}

fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
	close_guard::handle_event(app, &event);
	#[cfg(windows)]
	if matches!(event, tauri::RunEvent::Exit) {
		windows_launch::stop(app);
	}
	#[cfg(target_os = "macos")]
	if let tauri::RunEvent::Opened { urls } = event {
		open_requests::enqueue(
			app,
			urls.into_iter().filter_map(|url| url.to_file_path().ok()),
		);
	}
}

fn is_local_webview(url: &Url, development_origin: Option<&Url>) -> bool {
	let packaged = match (url.scheme(), url.host_str()) {
		("tauri", Some(LOCAL_WEBVIEW_HOST)) => url.port().is_none(),
		("http" | "https", Some(WINDOWS_WEBVIEW_HOST)) => url.port().is_none(),
		_ => false,
	};
	packaged
		|| (cfg!(not(feature = "custom-protocol"))
			&& development_origin.is_some_and(|origin| url.origin() == origin.origin()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn distinguishes_interactive_webview_origins() {
		let development_origin = Url::parse("http://127.0.0.1:1420").expect("valid dev origin");
		for url in ["tauri://localhost/index.html", "http://tauri.localhost/"] {
			let candidate = Url::parse(url).expect("valid packaged origin");
			assert!(is_local_webview(&candidate, Some(&development_origin)));
		}
		for url in [
			"https://example.test/",
			"http://tauri.localhost:8080/",
			"http://127.0.0.1:1421/",
			"tauri://not-localhost/",
			"file:///tmp/bank.SC2Bank",
		] {
			let candidate = Url::parse(url).expect("valid rejected origin");
			assert!(!is_local_webview(&candidate, Some(&development_origin)));
		}
		assert_eq!(
			is_local_webview(&development_origin, Some(&development_origin)),
			cfg!(not(feature = "custom-protocol"))
		);
	}
}
