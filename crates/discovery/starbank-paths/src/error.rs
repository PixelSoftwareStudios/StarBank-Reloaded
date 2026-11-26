use std::{error::Error, fmt, io, path::PathBuf};

/// Resource that stopped a scoped scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
	/// Number of visited filesystem entries.
	Entries,
	/// Directory nesting within the selected scope.
	Depth,
}

/// Failure encountered while checking an auto-detection candidate.
#[derive(Debug)]
pub struct CandidateFailure {
	/// Location checked, when the operating system provided one.
	pub path: Option<PathBuf>,
	/// Underlying error during discovery.
	pub error: PathError,
}

/// A path could not be resolved or scanned.
#[derive(Debug)]
#[non_exhaustive]
pub enum PathError {
	/// Automatic discovery has no implementation on this OS yet.
	NotImplemented,
	/// The current OS has no supported automatic discovery policy.
	UnsupportedPlatform,
	/// No candidate qualified as a StarCraft II data root.
	NotFound {
		/// Candidate errors beyond an ordinary missing path.
		diagnostics: Vec<CandidateFailure>,
	},
	/// At least one auto-detection candidate was inaccessible.
	PermissionDenied {
		/// Candidate failures that can explain the result.
		diagnostics: Vec<CandidateFailure>,
	},
	/// The path was neither a directory nor a regular bank file.
	InvalidSelection(PathBuf),
	/// A filesystem operation failed while reading a path.
	Io {
		/// Location where the operation failed.
		path: PathBuf,
		/// Original operating system error.
		source: io::Error,
	},
	/// The requested scope exceeded a scan limit.
	LimitReached {
		/// Resource that stopped the scan.
		kind: LimitKind,
		/// Location reached by the scan.
		path: PathBuf,
	},
	/// A directory entry was replaced during the scan.
	EntryChanged(PathBuf),
	/// Windows could not resolve its Documents known folder.
	#[cfg(windows)]
	KnownFolder(i32),
}

impl PathError {
	pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
		Self::Io {
			path: path.into(),
			source,
		}
	}

	#[cfg(any(test, target_os = "macos", windows))]
	pub(crate) fn is_permission_denied(&self) -> bool {
		matches!(self, Self::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied)
			|| matches!(self, Self::PermissionDenied { .. })
			|| self.is_known_folder_access_denied()
	}

	#[cfg(any(test, target_os = "macos", windows))]
	fn is_known_folder_access_denied(&self) -> bool {
		#[cfg(windows)]
		if let Self::KnownFolder(hresult) = self {
			return *hresult == windows_sys::Win32::Foundation::E_ACCESSDENIED;
		}
		false
	}
}

impl fmt::Display for PathError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::NotImplemented => {
				f.write_str("Automatic SC2 bank discovery is not implemented on this OS")
			}
			Self::UnsupportedPlatform => {
				f.write_str("Automatic SC2 bank discovery is unavailable on this OS")
			}
			Self::NotFound { .. } => {
				f.write_str("No SC2 Bank Folder path auto-detected, please select a folder")
			}
			Self::PermissionDenied { .. } => {
				f.write_str("An SC2 bank discovery location could not be accessed")
			}
			Self::InvalidSelection(path) => write!(
				f,
				"Select a bank file or directory instead of {}",
				path.display()
			),
			Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
			Self::LimitReached { kind, path } => {
				write!(f, "Scan {kind:?} limit reached at {}", path.display())
			}
			Self::EntryChanged(path) => write!(
				f,
				"Directory entry changed while reading {}",
				path.display()
			),
			#[cfg(windows)]
			Self::KnownFolder(hresult) => write!(
				f,
				"Windows Documents lookup failed: HRESULT {hresult:#010x}"
			),
		}
	}
}

impl Error for PathError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io { source, .. } => Some(source),
			_ => None,
		}
	}
}
