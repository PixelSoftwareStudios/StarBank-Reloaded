//! Reveals the file pinned to the active desktop document in the OS file manager.

use std::{
	path::Path,
	process::{Child, Command, Stdio},
};

use sc2bank::Limits;
use serde::Serialize;
use tauri::{AppHandle, Manager as _};

use crate::{
	commands::{CommandError, DesktopEngine},
	save::{ActiveTarget, DesktopSession},
};

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) enum RevealIssue {
	#[serde(rename = "reveal_unavailable")]
	Unavailable,
	#[serde(rename = "reveal_invalid_target")]
	InvalidTarget,
	#[serde(rename = "reveal_target_changed")]
	TargetChanged,
	#[serde(rename = "reveal_file_changed")]
	FileChanged,
	#[serde(rename = "reveal_failed")]
	Failed,
}

/// Reports whether this host has a supported native file manager.
#[tauri::command]
pub(crate) fn can_reveal_bank() -> bool {
	#[cfg(target_os = "macos")]
	let available = Path::new(FINDER_OPENER).is_file();

	#[cfg(windows)]
	let available = explorer_executable().is_some();

	#[cfg(not(any(target_os = "macos", windows)))]
	let available = false;

	available
}

/// Requests the OS to select the file identified by the current document and revision.
#[tauri::command]
pub(crate) async fn reveal_active_bank(
	app: AppHandle,
	target: ActiveTarget,
) -> Result<(), RevealIssue> {
	if !can_reveal_bank() {
		return Err(RevealIssue::Unavailable);
	}

	tauri::async_runtime::spawn_blocking(move || {
		let state = app.state::<DesktopEngine>();
		let child = {
			// Issue the native request while the selected document is still current.
			let session = state.engine.lock().map_err(|_| RevealIssue::Unavailable)?;
			launch(validated_path(&session, &target)?)?
		};
		finish_launch(child)
	})
	.await
	.map_err(|_| RevealIssue::Unavailable)?
}

fn validated_path<'session>(
	session: &'session DesktopSession,
	target: &ActiveTarget,
) -> Result<&'session Path, RevealIssue> {
	let (selected, _) = session.selected_for(target).map_err(|error| match error {
		CommandError::InvalidControl => RevealIssue::InvalidTarget,
		CommandError::ActiveWriteRejected(_) => RevealIssue::TargetChanged,
		_ => RevealIssue::Unavailable,
	})?;
	let path = &selected.path;
	let expected = selected
		.fingerprint
		.as_ref()
		.ok_or(RevealIssue::FileChanged)?;
	if !path.is_absolute() {
		return Err(RevealIssue::FileChanged);
	}
	let observed = starbank_fs::read_snapshot(path, Limits::default().document_bytes)
		.map_err(|_| RevealIssue::FileChanged)?;
	if observed.fingerprint() != expected {
		return Err(RevealIssue::FileChanged);
	}
	Ok(path)
}

fn launch(path: &Path) -> Result<Child, RevealIssue> {
	file_manager(path)
		.ok_or(RevealIssue::Unavailable)?
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.map_err(|_| RevealIssue::Failed)
}

#[cfg(target_os = "macos")]
const FINDER_OPENER: &str = "/usr/bin/open";
#[cfg(target_os = "macos")]
const FINDER_REVEAL_SWITCH: &str = "-R";

#[cfg(target_os = "macos")]
fn file_manager(path: &Path) -> Option<Command> {
	let mut command = Command::new(FINDER_OPENER);
	command.arg(FINDER_REVEAL_SWITCH).arg(path);
	Some(command)
}

#[cfg(target_os = "macos")]
fn finish_launch(mut child: Child) -> Result<(), RevealIssue> {
	match child.wait() {
		Ok(status) if status.success() => Ok(()),
		_ => Err(RevealIssue::Failed),
	}
}

#[cfg(windows)]
const WINDOWS_ROOT_ENV: &str = "SystemRoot";
#[cfg(windows)]
const EXPLORER_EXECUTABLE: &str = "explorer.exe";
#[cfg(windows)]
const EXPLORER_SELECT_SWITCH: &str = "/select,";

#[cfg(windows)]
fn explorer_executable() -> Option<std::path::PathBuf> {
	use std::os::windows::fs::MetadataExt as _;
	use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

	let root = std::env::var_os(WINDOWS_ROOT_ENV)?;
	let executable = explorer_path(Path::new(&root))?.join(EXPLORER_EXECUTABLE);
	let metadata = std::fs::symlink_metadata(&executable).ok()?;
	(metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0)
		.then_some(executable)
}

#[cfg(windows)]
fn explorer_path(path: &Path) -> Option<std::path::PathBuf> {
	use std::path::{Component, PathBuf, Prefix};

	let mut parts = path.components();
	let Component::Prefix(prefix) = parts.next()? else {
		return None;
	};
	let drive = match prefix.kind() {
		Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
		_ => return None,
	};
	if parts.next() != Some(Component::RootDir) {
		return None;
	}
	let mut result = PathBuf::from(format!("{}:\\", char::from(drive)));
	for part in parts {
		let Component::Normal(name) = part else {
			return None;
		};
		let encoded = name.as_encoded_bytes();
		if encoded
			.iter()
			.any(|&byte| byte < b' ' || byte == b'"' || byte == b':')
			|| encoded
				.last()
				.is_some_and(|byte| matches!(byte, b'.' | b' '))
		{
			return None;
		}
		result.push(name);
	}
	Some(result)
}

#[cfg(windows)]
fn explorer_select_arg(path: &Path) -> Option<std::ffi::OsString> {
	let path = explorer_path(path)?;
	let mut selection = std::ffi::OsString::from(EXPLORER_SELECT_SWITCH);
	selection.push("\"");
	selection.push(path);
	selection.push("\"");
	Some(selection)
}

#[cfg(windows)]
fn file_manager(path: &Path) -> Option<Command> {
	use std::os::windows::process::CommandExt as _;

	let mut command = Command::new(explorer_executable()?);
	// Explorer parses /select,"path" itself. The path validator rejects quote and control units.
	command.raw_arg(explorer_select_arg(path)?);
	Some(command)
}

#[cfg(windows)]
fn finish_launch(_child: Child) -> Result<(), RevealIssue> {
	// Explorer may hand off to an existing manager or stay alive as the manager.
	Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn file_manager(_path: &Path) -> Option<Command> {
	None
}

#[cfg(not(any(target_os = "macos", windows)))]
fn finish_launch(_child: Child) -> Result<(), RevealIssue> {
	Err(RevealIssue::Unavailable)
}

#[cfg(test)]
mod tests;
