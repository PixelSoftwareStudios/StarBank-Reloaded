use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};

/// The previously observed identity, content digest and change stamp for a local file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
	#[cfg(any(target_os = "linux", target_os = "macos"))]
	pub(crate) observed: crate::native::ObservedFile,
	#[cfg(any(target_os = "linux", target_os = "macos"))]
	pub(crate) directory: crate::native::FileIdentity,
	#[cfg(any(target_os = "linux", target_os = "macos"))]
	pub(crate) name: std::ffi::OsString,
	#[cfg(windows)]
	pub(crate) windows: crate::windows::ObservedPath,
	pub(crate) sha256: [u8; 32],
}

impl Fingerprint {
	/// The SHA-256 of the observed main data stream.
	#[must_use]
	pub fn sha256(&self) -> &[u8; 32] {
		&self.sha256
	}
}

/// A bounded read paired with its native conflict token.
#[derive(Debug)]
pub struct FileSnapshot {
	pub(crate) bytes: Vec<u8>,
	pub(crate) fingerprint: Fingerprint,
}

impl FileSnapshot {
	/// Borrows the exact observed bytes.
	#[must_use]
	pub fn bytes(&self) -> &[u8] {
		&self.bytes
	}

	/// Takes ownership of the observed bytes.
	#[must_use]
	pub fn into_bytes(self) -> Vec<u8> {
		self.bytes
	}

	/// Borrows the conflict token for the observed target.
	#[must_use]
	pub fn fingerprint(&self) -> &Fingerprint {
		&self.fingerprint
	}
}

/// The caller's explicit publication decision.
#[derive(Clone, Copy, Debug)]
pub enum CommitIntent<'expected> {
	/// Publish only when the destination is absent.
	CreateNew,
	/// Replace only the target previously observed by the caller.
	ReplaceExpected(&'expected Fingerprint),
}

/// The terminal state of a commit attempt.
#[derive(Debug)]
#[must_use]
pub enum CommitOutcome {
	/// Publication, available sync, and read-back verification succeeded.
	CommittedVerified(CommitReceipt),
	/// The attempt did not publish any bytes at the destination.
	RejectedUnchanged(StorageError),
	/// Publication or verification was ambiguous. Reopen and inspect before retrying.
	CommitUncertain(CommitIssue),
}

/// Evidence of a verified publication.
#[derive(Debug)]
pub struct CommitReceipt {
	pub(crate) fingerprint: Fingerprint,
	pub(crate) recovery: Option<RecoveryArtifact>,
}

impl CommitReceipt {
	/// The fingerprint of the published content.
	#[must_use]
	pub fn fingerprint(&self) -> &Fingerprint {
		&self.fingerprint
	}

	/// The displaced file retained after a verified replacement.
	#[must_use]
	pub fn recovery(&self) -> Option<&RecoveryArtifact> {
		self.recovery.as_ref()
	}
}

/// A pending or displaced sibling retained for explicit recovery.
#[derive(Debug)]
pub struct RecoveryArtifact {
	pub(crate) path: PathBuf,
}

impl RecoveryArtifact {
	/// The sibling path at the time of the attempt. Inspect before operating on it.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}
}

/// Details for an attempt with a potentially changed destination.
#[derive(Debug)]
pub struct CommitIssue {
	pub(crate) error: StorageError,
	pub(crate) recovery: RecoveryArtifact,
}

impl CommitIssue {
	/// The original failure or conflict.
	#[must_use]
	pub fn error(&self) -> &StorageError {
		&self.error
	}

	/// The sibling location reserved for the transaction. It may have already moved.
	#[must_use]
	pub fn recovery(&self) -> &RecoveryArtifact {
		&self.recovery
	}
}

/// A storage failure before or after publication.
#[derive(Debug)]
pub enum StorageError {
	/// The current target or its parent differs from the observed state.
	Conflict,
	/// The operation requires a supported platform or filesystem.
	Unsupported(&'static str),
	/// The path is outside the safe path policy.
	UnsafePath(&'static str),
	/// The target contains metadata the writer cannot preserve safely.
	UnsafeMetadata(&'static str),
	/// The provided limit is smaller than the file or prepared bytes.
	TooLarge { max_bytes: usize },
	/// A system operation failed.
	Io {
		operation: &'static str,
		source: io::Error,
	},
}

impl StorageError {
	/// Maps the error to a standard I/O category.
	#[must_use]
	pub fn kind(&self) -> io::ErrorKind {
		match self {
			Self::Conflict => io::ErrorKind::ResourceBusy,
			Self::Unsupported(_) => io::ErrorKind::Unsupported,
			Self::UnsafePath(_) | Self::UnsafeMetadata(_) => io::ErrorKind::PermissionDenied,
			Self::TooLarge { .. } => io::ErrorKind::FileTooLarge,
			Self::Io { source, .. } => source.kind(),
		}
	}

	#[cfg(any(target_os = "linux", target_os = "macos", windows))]
	pub(crate) fn io(operation: &'static str, source: impl Into<io::Error>) -> Self {
		Self::Io {
			operation,
			source: source.into(),
		}
	}
}

impl Display for StorageError {
	fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Self::Conflict => out.write_str("file changed since it was observed"),
			Self::Unsupported(reason) => write!(out, "storage operation unsupported ({reason})"),
			Self::UnsafePath(reason) => write!(out, "file path refused ({reason})"),
			Self::UnsafeMetadata(reason) => write!(out, "file metadata refused ({reason})"),
			Self::TooLarge { max_bytes } => write!(out, "file exceeds the {max_bytes}-byte limit"),
			Self::Io { operation, source } => write!(out, "{operation} failed ({source})"),
		}
	}
}

impl Error for StorageError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Io { source, .. } => Some(source),
			_ => None,
		}
	}
}
