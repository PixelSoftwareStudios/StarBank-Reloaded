#[cfg(any(test, windows, target_os = "macos"))]
use std::{
	fs, io,
	path::{Path, PathBuf},
};

use crate::{CandidateFailure, PathError, PathSelection, ScanLimits};

#[cfg(any(test, windows, target_os = "macos"))]
use crate::{context::GAME_DIRECTORY, selection::probe_game_root};
#[cfg(any(test, windows))]
const DOCUMENTS: &str = "Documents";
#[cfg(any(test, windows))]
const ONE_DRIVE: &str = "OneDrive";

/// The first qualifying game data root and relevant candidate diagnostics.
#[derive(Debug)]
pub struct AutoDetectedRoot {
	/// Selected root and all its bank folders. Empty roots are a distinct state.
	pub selection: PathSelection,
	/// Errors encountered while checking other candidates.
	pub diagnostics: Vec<CandidateFailure>,
}

/// Finds the first platform candidate with Banks folders or a marker-only fallback.
///
/// Linux returns `NotImplemented`; it supports manual selections independently.
///
/// # Errors
///
/// Reports when discovery is unavailable, no root qualifies, or candidate
/// permissions or limits prevent a conclusive search.
pub fn get_sc2_root_path(limits: ScanLimits) -> Result<AutoDetectedRoot, PathError> {
	platform_root(limits)
}

#[cfg(target_os = "macos")]
fn platform_root(limits: ScanLimits) -> Result<AutoDetectedRoot, PathError> {
	let home = std::env::home_dir()
		.filter(|path| path.is_absolute())
		.ok_or_else(|| PathError::NotFound {
			diagnostics: Vec::new(),
		})?;
	resolve_candidates(mac_candidates(&home).map(Ok), limits)
}

#[cfg(any(test, target_os = "macos"))]
fn mac_candidates(home: &std::path::Path) -> impl Iterator<Item = PathBuf> {
	[
		"Library/Application Support/Blizzard",
		"Documents",
		"Library/Mobile Documents/com~apple~CloudDocs/Documents",
	]
	.into_iter()
	.map(|relative| home.join(relative).join(GAME_DIRECTORY))
}

#[cfg(windows)]
fn platform_root(limits: ScanLimits) -> Result<AutoDetectedRoot, PathError> {
	use std::env;
	let profile = env::var_os("USERPROFILE")
		.map(PathBuf::from)
		.filter(|path| path.is_absolute());
	let cloud = [ONE_DRIVE, "OneDriveConsumer", "OneDriveCommercial"]
		.into_iter()
		.filter_map(env::var_os)
		.map(PathBuf::from)
		.filter(|path| path.is_absolute());
	let known_documents = crate::windows::documents();
	resolve_candidates(windows_candidates(known_documents, profile, cloud), limits)
}

#[cfg(any(test, windows))]
fn windows_candidates(
	known_documents: Result<PathBuf, PathError>,
	profile: Option<PathBuf>,
	clouds: impl Iterator<Item = PathBuf>,
) -> impl Iterator<Item = Result<PathBuf, PathError>> {
	let conventional = profile
		.as_ref()
		.map(|profile| profile.join(ONE_DRIVE).join(DOCUMENTS).join(GAME_DIRECTORY));
	let fallback = profile.map(|profile| profile.join(DOCUMENTS).join(GAME_DIRECTORY));
	std::iter::once(known_documents.map(|path| path.join(GAME_DIRECTORY)))
		.chain(fallback.into_iter().map(Ok))
		.chain(clouds.map(|path| Ok(path.join(DOCUMENTS).join(GAME_DIRECTORY))))
		.chain(conventional.into_iter().map(Ok))
}

#[cfg(target_os = "linux")]
fn platform_root(_limits: ScanLimits) -> Result<AutoDetectedRoot, PathError> {
	Err(PathError::NotImplemented)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_root(_limits: ScanLimits) -> Result<AutoDetectedRoot, PathError> {
	Err(PathError::UnsupportedPlatform)
}

#[cfg(any(test, windows, target_os = "macos"))]
fn resolve_candidates(
	candidates: impl IntoIterator<Item = Result<PathBuf, PathError>>,
	limits: ScanLimits,
) -> Result<AutoDetectedRoot, PathError> {
	let mut diagnostics = Vec::new();
	let mut seen = Vec::new();
	let mut bankless = None;
	for candidate in candidates {
		let path = match candidate {
			Ok(path) => path,
			Err(error) => {
				diagnostics.push(CandidateFailure { path: None, error });
				continue;
			}
		};
		let canonical = match canonical_candidate(&path, |candidate| fs::canonicalize(candidate)) {
			Ok(Some(canonical)) => canonical,
			Ok(None) => continue,
			Err(error) => {
				diagnostics.push(CandidateFailure {
					path: Some(path),
					error,
				});
				continue;
			}
		};
		if seen.contains(&canonical) {
			continue;
		}
		seen.push(canonical.clone());
		match probe_game_root(canonical, limits) {
			Ok(Some(selection)) if !selection.bank_roots().is_empty() => {
				return Ok(AutoDetectedRoot {
					selection,
					diagnostics,
				});
			}
			Ok(Some(selection)) if bankless.is_none() => bankless = Some(selection),
			Ok(_) => {}
			Err(error) => diagnostics.push(CandidateFailure {
				path: Some(path),
				error,
			}),
		}
	}
	if let Some(selection) = bankless {
		return Ok(AutoDetectedRoot {
			selection,
			diagnostics,
		});
	}
	if diagnostics
		.iter()
		.any(|failure| failure.error.is_permission_denied())
	{
		return Err(PathError::PermissionDenied { diagnostics });
	}
	if let Some(index) = diagnostics
		.iter()
		.position(|failure| matches!(failure.error, PathError::LimitReached { .. }))
	{
		return Err(diagnostics.swap_remove(index).error);
	}
	Err(PathError::NotFound { diagnostics })
}

#[cfg(any(test, windows, target_os = "macos"))]
fn canonical_candidate(
	path: &Path,
	canonicalize: impl FnOnce(&Path) -> io::Result<PathBuf>,
) -> Result<Option<PathBuf>, PathError> {
	#[cfg(windows)]
	check_windows_candidate(path)?;
	let canonical = match canonicalize(path) {
		Ok(canonical) => canonical,
		Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
		Err(source) => return Err(PathError::io(path, source)),
	};
	// A drive path may resolve through a link to a path in another namespace.
	// Refuse that target before probing its account and bank directories.
	#[cfg(windows)]
	check_windows_candidate(&canonical)?;
	Ok(Some(canonical))
}

#[cfg(windows)]
fn check_windows_candidate(path: &Path) -> Result<(), PathError> {
	use std::path::{Component, Prefix};

	let drive = matches!(path.components().next(),
		Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
	);
	if path.is_absolute()
		&& drive
		&& path.components().all(|component| match component {
			Component::ParentDir => false,
			Component::Normal(name) => !name.as_encoded_bytes().contains(&b':'),
			_ => true,
		}) {
		Ok(())
	} else {
		Err(PathError::UnsupportedCandidate)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn mac_root_order_with_early_exit_and_marker_only_fallback()
	-> Result<(), Box<dyn std::error::Error>> {
		let temp = tempfile::tempdir()?;
		let paths = mac_candidates(temp.path()).collect::<Vec<_>>();
		assert_eq!(
			paths[0],
			temp.path()
				.join("Library/Application Support/Blizzard/StarCraft II")
		);
		assert_eq!(paths[1], temp.path().join("Documents/StarCraft II"));
		assert_eq!(
			paths[2],
			temp.path()
				.join("Library/Mobile Documents/com~apple~CloudDocs/Documents/StarCraft II")
		);
		fs::create_dir_all(&paths[0])?;
		fs::write(paths[0].join("Variables.txt"), b"")?;
		let fallback =
			resolve_candidates(mac_candidates(temp.path()).map(Ok), ScanLimits::default())?;
		assert_eq!(fallback.selection.path(), paths[0].canonicalize()?);
		assert!(fallback.selection.bank_roots().is_empty());

		fs::create_dir_all(paths[1].join("Accounts/999/1-S2-1-987/Banks"))?;
		let (initial, remainder) = paths.split_at(2);
		let candidates = initial
			.iter()
			.cloned()
			.map(Ok)
			.chain(std::iter::from_fn(|| {
				panic!("lower-priority candidate must not be probed after a valid root");
			}));
		let detected = resolve_candidates(candidates, ScanLimits::default())?;
		assert_eq!(detected.selection.path(), paths[1].canonicalize()?);
		assert_eq!(detected.selection.bank_roots().len(), 1);
		assert_eq!(remainder.len(), 1);
		Ok(())
	}

	#[cfg(unix)]
	#[test]
	fn automatic_candidate_preserves_marker_only_fallback_after_alias_resolution()
	-> Result<(), Box<dyn std::error::Error>> {
		let temp = tempfile::tempdir()?;
		let target = temp.path().join("Renamed game data");
		fs::create_dir(&target)?;
		fs::write(target.join("ExecuteInfo.txt"), b"")?;
		let alias = temp.path().join(GAME_DIRECTORY);
		std::os::unix::fs::symlink(&target, &alias)?;
		let discovered = resolve_candidates([Ok(alias)], ScanLimits::default())?;
		assert_eq!(discovered.selection.path(), target.canonicalize()?);
		assert!(discovered.selection.bank_roots().is_empty());
		Ok(())
	}

	#[test]
	fn windows_prefers_redirected_documents_then_explicit_onedrive_fallbacks()
	-> Result<(), Box<dyn std::error::Error>> {
		let temp = tempfile::tempdir()?;
		let redirect = temp.path().join("Redirected Documents");
		let profile = temp.path().join("Profile");
		let cloud = [
			temp.path().join("Personal"),
			temp.path().join("Consumer"),
			temp.path().join("Company"),
		];
		let candidates = windows_candidates(
			Ok(redirect.clone()),
			Some(profile.clone()),
			cloud.iter().cloned(),
		)
		.collect::<Result<Vec<_>, _>>()?;
		assert_eq!(
			candidates,
			[
				redirect.join(GAME_DIRECTORY),
				profile.join(DOCUMENTS).join(GAME_DIRECTORY),
				cloud[0].join(DOCUMENTS).join(GAME_DIRECTORY),
				cloud[1].join(DOCUMENTS).join(GAME_DIRECTORY),
				cloud[2].join(DOCUMENTS).join(GAME_DIRECTORY),
				profile.join(ONE_DRIVE).join(DOCUMENTS).join(GAME_DIRECTORY),
			]
		);
		fs::create_dir_all(redirect.join(GAME_DIRECTORY).join("Banks"))?;
		fs::create_dir_all(profile.join(DOCUMENTS).join(GAME_DIRECTORY).join("Banks"))?;
		let detected = resolve_candidates(candidates.into_iter().map(Ok), ScanLimits::default())?;
		assert_eq!(
			detected.selection.path(),
			redirect.join(GAME_DIRECTORY).canonicalize()?
		);
		Ok(())
	}

	#[cfg(windows)]
	#[test]
	fn automatic_policy_rejects_windows_network_device_and_ambiguous_candidates() {
		for candidate in [
			r"\\fileserver\Users\Documents\StarCraft II",
			r"\\?\UNC\fileserver\Users\Documents\StarCraft II",
			r"\\.\C:\Documents\StarCraft II",
			r"\\.\PIPE\Documents\StarCraft II",
			r"\\?\GLOBALROOT\Device\Mup\fileserver\Users\Documents",
			r"\\?\Volume{11223344-0000-0000-0000-000000000000}\Documents",
			r"C:Documents\StarCraft II",
			r"\Documents\StarCraft II",
			r"Documents\StarCraft II",
			r"C:\Documents\..\Other\StarCraft II",
			r"C:\Documents:stream\StarCraft II",
		] {
			assert!(
				matches!(
					canonical_candidate(Path::new(candidate), |_| panic!(
						"rejected path was probed"
					)),
					Err(PathError::UnsupportedCandidate)
				),
				"{candidate}"
			);
		}
	}

	#[cfg(windows)]
	#[test]
	fn automatic_policy_retains_ordinary_and_verbatim_drive_paths() {
		for candidate in [
			r"C:\Users\Local\Documents\StarCraft II",
			r"D:\OneDrive - Example\Documents\StarCraft II",
			r"\\?\E:\Redirected Documents\StarCraft II",
		] {
			assert!(
				check_windows_candidate(Path::new(candidate)).is_ok(),
				"{candidate}"
			);
		}
	}

	#[cfg(windows)]
	#[test]
	fn resolver_rejects_a_resolved_target_in_the_network_namespace() {
		let drive_path = Path::new(r"C:\Documents\StarCraft II");
		let resolved = PathBuf::from(r"\\?\UNC\fileserver\Users\Documents\StarCraft II");
		assert!(matches!(
			canonical_candidate(drive_path, |_| Ok(resolved)),
			Err(PathError::UnsupportedCandidate)
		));
	}

	#[test]
	fn inaccessible_candidate_is_not_missing() {
		let denied = PathError::io(
			"synthetic",
			io::Error::from(io::ErrorKind::PermissionDenied),
		);
		let result = resolve_candidates([Err(denied)], ScanLimits::default());
		assert!(
			matches!(result, Err(PathError::PermissionDenied { diagnostics }) if diagnostics.len() == 1)
		);
	}
}
