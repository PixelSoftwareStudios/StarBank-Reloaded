fn main() -> Result<(), Box<dyn std::error::Error>> {
	#[cfg(feature = "desktop")]
	tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
		tauri_build::AppManifest::new().commands(&[
			"open_bank",
			"dispatch_bank",
			"export_bank_copy",
			"can_export_bank",
		]),
	))?;
	Ok(())
}
