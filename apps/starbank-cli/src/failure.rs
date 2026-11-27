use std::{fmt, io, path::Path, process::ExitCode};

use sc2bank::{Location, ParseError, ParseErrorKind};
use serde::Serialize;
use starbank_editor::EditorError;
use starbank_fs::StorageError;
use starbank_paths::PathError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
	Success = 0,
	Usage = 2,
	Validation = 3,
	Io = 4,
	Conflict = 5,
	Uncertain = 6,
	Unsupported = 7,
}

impl From<Status> for ExitCode {
	fn from(status: Status) -> Self {
		Self::from(status as u8)
	}
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorCode {
	InvalidArguments,
	InvalidBank,
	UnsupportedBankFormat,
	InputLimit,
	InvalidContext,
	InvalidPatch,
	UnsupportedPatchVersion,
	UnsupportedStorage,
	InputChanged,
	DestinationExists,
	InvalidEdit,
	CannotSign,
	StaleSignature,
	InvalidBackup,
	BackupChanged,
	BackupMetadata,
	BackupUncertain,
	Io,
	DiscoveryUnavailable,
	DiscoveryFailed,
	InvalidMapSelection,
	InvalidMapArchive,
	InvalidMapMetadata,
	UnsupportedMapArchive,
	MapScanFailed,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticLocation {
	pub(crate) byte: Option<usize>,
	pub(crate) line: u32,
	pub(crate) column: u32,
}

impl From<Location> for DiagnosticLocation {
	fn from(value: Location) -> Self {
		Self {
			byte: value.byte,
			line: value.line,
			column: value.column,
		}
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct Failure {
	#[serde(skip)]
	pub(crate) status: Status,
	pub(crate) code: ErrorCode,
	pub(crate) message: &'static str,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) location: Option<DiagnosticLocation>,
	#[serde(skip_serializing_if = "Option::is_none")]
	backup_path: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	recovery_path: Option<String>,
}

impl Failure {
	pub(crate) fn new(status: Status, code: ErrorCode, message: &'static str) -> Self {
		Self {
			status,
			code,
			message,
			location: None,
			backup_path: None,
			recovery_path: None,
		}
	}

	pub(crate) fn has_artifact(&self) -> bool {
		self.backup_path.is_some() || self.recovery_path.is_some()
	}

	pub(crate) fn with_backup_path(mut self, path: &Path) -> Self {
		self.backup_path = Some(path.to_string_lossy().into_owned());
		self
	}

	pub(crate) fn with_recovery_path(mut self, path: &Path) -> Self {
		self.recovery_path = Some(path.to_string_lossy().into_owned());
		self
	}
}

impl fmt::Display for Failure {
	fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(output, "{}", self.message)?;
		if let Some(location) = &self.location {
			write!(
				output,
				" (line {}, column {})",
				location.line, location.column
			)?;
		}
		if let Some(path) = &self.backup_path {
			write!(output, " Snapshot at {path:?}.")?;
		}
		if let Some(path) = &self.recovery_path {
			write!(
				output,
				" Inspect recovery location {path:?} before retrying."
			)?;
		}
		Ok(())
	}
}

impl From<io::Error> for Failure {
	fn from(error: io::Error) -> Self {
		let message = match error.kind() {
			io::ErrorKind::NotFound => "The selected file was not found.",
			io::ErrorKind::PermissionDenied => "Access to the selected file was denied.",
			_ => "The selected file could not be read.",
		};
		Self::new(Status::Io, ErrorCode::Io, message)
	}
}

impl From<ParseError> for Failure {
	fn from(error: ParseError) -> Self {
		let (status, code, message) = match error.kind() {
			ParseErrorKind::UnsupportedEncoding | ParseErrorKind::UnsupportedXmlVersion => (
				Status::Unsupported,
				ErrorCode::UnsupportedBankFormat,
				"The bank encoding or XML version is unsupported.",
			),
			ParseErrorKind::LimitExceeded(_) => (
				Status::Validation,
				ErrorCode::InputLimit,
				"The bank exceeded its XML resource limits.",
			),
			_ => (
				Status::Validation,
				ErrorCode::InvalidBank,
				"The file is not valid bank XML.",
			),
		};
		Self {
			status,
			code,
			message,
			location: error.location().map(DiagnosticLocation::from),
			backup_path: None,
			recovery_path: None,
		}
	}
}

impl From<PathError> for Failure {
	fn from(value: PathError) -> Self {
		match value {
			PathError::NotImplemented | PathError::UnsupportedPlatform => Self::new(
				Status::Unsupported,
				ErrorCode::DiscoveryUnavailable,
				"Automatic discovery is unavailable on this platform. Select a folder with --root.",
			),
			_ => Self::new(
				Status::Io,
				ErrorCode::DiscoveryFailed,
				"The selected bank locations could not be found or read.",
			),
		}
	}
}

impl From<EditorError> for Failure {
	fn from(error: EditorError) -> Self {
		match error {
			EditorError::Parse(error) => error.into(),
			EditorError::StaleRevision { .. } => Self::new(
				Status::Conflict,
				ErrorCode::InputChanged,
				"The bank changed since the requested operation was prepared.",
			),
			EditorError::Signing(_) => Self::new(
				Status::Validation,
				ErrorCode::CannotSign,
				"The bank could not be signed and verified for the destination.",
			),
			_ => Self::new(
				Status::Validation,
				ErrorCode::InvalidEdit,
				"The document could not accept one or more requested edits.",
			),
		}
	}
}

impl From<StorageError> for Failure {
	fn from(error: StorageError) -> Self {
		match error {
			StorageError::Conflict => Self::new(
				Status::Conflict,
				ErrorCode::InputChanged,
				"The observed file or its parent directory changed.",
			),
			StorageError::TooLarge { .. } => Self::new(
				Status::Validation,
				ErrorCode::InputLimit,
				"The selected file exceeded its byte limit.",
			),
			StorageError::Unsupported(_) => Self::new(
				Status::Unsupported,
				ErrorCode::UnsupportedStorage,
				"This storage operation is unavailable on the current platform or filesystem.",
			),
			StorageError::UnsafePath(_) | StorageError::UnsafeMetadata(_) => Self::new(
				Status::Io,
				ErrorCode::Io,
				"The selected path or its file metadata could not be handled safely.",
			),
			StorageError::Io { source, .. } if source.kind() == io::ErrorKind::AlreadyExists => {
				Self::new(
					Status::Conflict,
					ErrorCode::DestinationExists,
					"The destination already exists.",
				)
			}
			StorageError::Io { source, .. } => source.into(),
		}
	}
}
