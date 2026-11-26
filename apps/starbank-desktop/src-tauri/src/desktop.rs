use tauri::{Url, WebviewWindowBuilder, webview::NewWindowResponse};

use crate::commands::DesktopEngine;

const MAIN_WINDOW: &str = "main";
const LOCAL_WEBVIEW_HOST: &str = "localhost";
const WINDOWS_WEBVIEW_HOST: &str = "tauri.localhost";

pub(crate) fn run() -> tauri::Result<()> {
	tauri::Builder::default()
		.manage(DesktopEngine::default())
		.setup(|app| {
			let config = app
				.config()
				.app
				.windows
				.iter()
				.find(|window| window.label == MAIN_WINDOW)
				.ok_or("missing main webview configuration")?;
			let development_origin = app.config().build.dev_url.clone();
			WebviewWindowBuilder::from_config(app, config)?
				.on_navigation(move |url| is_local_webview(url, development_origin.as_ref()))
				.on_new_window(|_, _| NewWindowResponse::Deny)
				.build()?;
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![
			crate::commands::open_bank,
			crate::commands::dispatch_bank,
			crate::export::export_bank_copy,
			crate::export::can_export_bank
		])
		.run(tauri::generate_context!())
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
