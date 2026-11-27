use std::{error::Error, fmt};

use crate::MpqError;

/// An input that cannot be used for a verified V1 append.
#[derive(Debug)]
#[non_exhaustive]
pub enum V1AppendError {
	/// An input or output could not be parsed under the archive reader's bounds.
	Archive(MpqError),
	/// An input or output did not match the strict append profile.
	Layout(V1LayoutIssue),
	/// A proposed addition cannot be represented in the limited append profile.
	Addition(V1AdditionIssue),
	/// The candidate changed data beyond the authorized additions.
	Preservation(V1PreservationIssue),
}

/// A classic archive condition outside the verified append profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum V1LayoutIssue {
	/// The archive is not flat MPQ V1.
	Version,
	/// Data follows the declared classic archive end.
	Trailer,
	/// Internal bookkeeping members could be rewritten or invalidated by an append.
	InternalMember,
	/// The archive includes unsupported member flags or reserved hash fields.
	Flags,
	/// The classic table includes a deleted, unreferenced or multiply referenced block.
	BlockReferences,
	/// Packed member ranges are not bounded, nonoverlapping and contiguous.
	PayloadRanges,
}

/// A rejected addition proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum V1AdditionIssue {
	/// The proposal has zero components or exceeds the component count cap.
	Count,
	/// A component is empty or the component or aggregate size exceeds its cap.
	ComponentSize,
	/// The projected candidate could exceed the archive byte cap.
	ArchiveSize,
	/// A proposed archive member name is invalid, reserved or duplicated.
	Name,
	/// A component's proposed name is already present under some locale.
	ExistingMember,
	/// The existing tables lack capacity for the requested entries.
	Capacity,
}

/// A discrepancy found during verification against the original archive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum V1PreservationIssue {
	/// The sector size or number of hash slots changed.
	Geometry,
	/// An old hash or block-table entry changed, or an unexpected hash entry appeared.
	RetainedTable,
	/// The candidate changed the packed data bytes of original member(s).
	RetainedPayload,
	/// A proposed member lookup, stored extent or payload differs from the plan.
	AddedMember,
}

impl fmt::Display for V1AppendError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Archive(error) => write!(formatter, "could not read classic archive: {error}"),
			Self::Layout(issue) => write!(
				formatter,
				"archive outside the V1 append profile: {issue:?}"
			),
			Self::Addition(issue) => write!(formatter, "V1 append proposal refused: {issue:?}"),
			Self::Preservation(issue) => {
				write!(formatter, "V1 append verification failed: {issue:?}")
			}
		}
	}
}

impl Error for V1AppendError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::Archive(error) => Some(error),
			_ => None,
		}
	}
}

impl From<MpqError> for V1AppendError {
	fn from(error: MpqError) -> Self {
		Self::Archive(error)
	}
}

impl From<V1LayoutIssue> for V1AppendError {
	fn from(issue: V1LayoutIssue) -> Self {
		Self::Layout(issue)
	}
}

impl From<V1AdditionIssue> for V1AppendError {
	fn from(issue: V1AdditionIssue) -> Self {
		Self::Addition(issue)
	}
}

impl From<V1PreservationIssue> for V1AppendError {
	fn from(issue: V1PreservationIssue) -> Self {
		Self::Preservation(issue)
	}
}
