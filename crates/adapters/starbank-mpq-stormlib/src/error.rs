use std::error::Error;
use std::fmt;
use std::io;

use starbank_mpq::V1AppendError;

/// Archive calls whose failures prevent releasing the scratch candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeOperation {
	/// Open the adapter's private archive.
	Open,
	/// Check whether the plan's new archive member already exists.
	CheckMember,
	/// Create a stored archive member.
	CreateMember,
	/// Write the original component bytes.
	WriteMember,
	/// Finalize and release a writable member.
	FinishMember,
	/// Flush archive members and tables to the scratch file.
	Flush,
	/// Close the archive and release its handle.
	Close,
}

/// Filesystem operations limited to the adapter's private storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScratchOperation {
	/// Open and inspect the fixed scratch volume.
	CheckVolume,
	/// Create a private scratch directory.
	CreateDirectory,
	/// Create the scratch file exclusively.
	CreateFile,
	/// Copy the original bytes to scratch.
	WriteSource,
	/// Read the completed scratch candidate.
	ReadCandidate,
	/// Delete the private scratch directory and contents.
	RemoveDirectory,
}

/// Failure to produce an independently verified archive candidate.
#[derive(Debug)]
pub enum AppendError {
	/// The fixed scratch root must be a trusted local APFS directory.
	UnsupportedScratch,
	/// The generated scratch path could not be represented as a local StormLib path.
	InvalidScratchPath,
	/// A previous native operation unwound while holding the exclusive library guard.
	NativeStateUnavailable,
	/// An expected private filesystem operation failed.
	Scratch {
		/// The failed operation.
		operation: ScratchOperation,
		/// The original I/O error.
		source: io::Error,
	},
	/// The requested member already exists.
	ExistingMember(String),
	/// The component cannot be represented by the native signature.
	InvalidMember(String),
	/// The native library reported failure.
	Native {
		/// The failed ABI call.
		operation: NativeOperation,
		/// The requested member, when applicable.
		member: Option<String>,
		/// The StormLib error captured immediately after failure.
		code: u32,
	},
	/// A successful native call did not return a non-null handle.
	MissingHandle {
		/// The ABI operation expected to return a handle.
		operation: NativeOperation,
	},
	/// The candidate exceeds the verified archive byte limit.
	CandidateTooLarge,
	/// The private candidate length changed during readback.
	CandidateChanged,
	/// The candidate failed independent validation against the original plan.
	Verification(V1AppendError),
	/// Candidate creation and subsequent private scratch cleanup both failed.
	Cleanup {
		/// The failure that prevented returning a candidate.
		primary: Box<AppendError>,
		/// The failure encountered while removing private scratch.
		cleanup: Box<AppendError>,
	},
}

impl AppendError {
	pub(crate) fn scratch(operation: ScratchOperation, source: impl Into<io::Error>) -> Self {
		Self::Scratch {
			operation,
			source: source.into(),
		}
	}
}

impl fmt::Display for AppendError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnsupportedScratch => f.write_str("archive scratch requires trusted local APFS"),
			Self::InvalidScratchPath => {
				f.write_str("generated archive scratch path is unsupported")
			}
			Self::NativeStateUnavailable => f.write_str("exclusive StormLib state is unavailable"),
			Self::Scratch { operation, source } => {
				write!(f, "archive scratch {operation:?}: {source}")
			}
			Self::ExistingMember(name) => write!(f, "archive member already exists: {name}"),
			Self::InvalidMember(name) => write!(f, "invalid native archive member: {name}"),
			Self::Native {
				operation,
				member,
				code,
			} => {
				write!(f, "StormLib {operation:?}")?;
				if let Some(name) = member {
					write!(f, " for {name}")?;
				}
				write!(f, " failed with native error {code}")
			}
			Self::MissingHandle { operation } => {
				write!(f, "StormLib {operation:?} returned no handle")
			}
			Self::CandidateTooLarge => f.write_str("archive candidate exceeds its size limit"),
			Self::CandidateChanged => {
				f.write_str("archive candidate length changed during readback")
			}
			Self::Verification(error) => {
				write!(f, "archive candidate failed verification: {error}")
			}
			Self::Cleanup { primary, cleanup } => {
				write!(
					f,
					"{primary}; private scratch cleanup also failed: {cleanup}"
				)
			}
		}
	}
}

impl Error for AppendError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Scratch { source, .. } => Some(source),
			Self::Verification(error) => Some(error),
			Self::Cleanup { primary, .. } => Some(primary.as_ref()),
			_ => None,
		}
	}
}
