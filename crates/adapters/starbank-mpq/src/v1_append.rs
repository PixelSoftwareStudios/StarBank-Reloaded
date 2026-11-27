use std::{collections::BTreeSet, fmt};

use stormlib_rs::{BlockTableEntry, MpqFileFlags, lookup_file_name};

use crate::header::MAX_ARCHIVE_BYTES;

mod error;
mod layout;

pub use error::{V1AdditionIssue, V1AppendError, V1LayoutIssue, V1PreservationIssue};
use layout::{Layout, is_live, packed_range};

/// Maximum archive length accepted by the V1 append profile.
pub const V1_APPEND_ARCHIVE_LIMIT: u64 = MAX_ARCHIVE_BYTES;

const MAX_COMPONENTS: usize = 16;
const MAX_COMPONENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_COMPONENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_MEMBER_NAME_BYTES: usize = 240;

/// Explicit member name and exact stored payload proposed for append.
#[derive(Clone, Copy, Debug)]
pub struct V1Addition<'a> {
	/// Native MPQ name with backslashes between portable path components.
	pub name: &'a str,
	/// Nonempty bytes to add without modifying or extracting any prior member.
	pub bytes: &'a [u8],
}

/// An append proposal bound to its immutable source archive and additions.
///
/// Verification covers flat classic MPQ V1 storage. SC2 component semantics,
/// provenance and file authorization belong to the caller.
pub struct V1AppendPlan<'a> {
	source: &'a [u8],
	additions: &'a [V1Addition<'a>],
	original: Layout,
}

impl fmt::Debug for V1AppendPlan<'_> {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("V1AppendPlan")
			.field("source_bytes", &self.source.len())
			.field("addition_count", &self.additions.len())
			.finish_non_exhaustive()
	}
}

impl<'a> V1AppendPlan<'a> {
	/// Checks a source and its proposed additions under the classic V1 bounds.
	///
	/// # Errors
	///
	/// Rejects malformed or out-of-profile archives, unsafe names or sizes,
	/// existing member names at any locale, and proposals requiring table growth.
	pub fn prepare(
		source: &'a [u8],
		additions: &'a [V1Addition<'a>],
	) -> Result<Self, V1AppendError> {
		validate_additions(source.len(), additions)?;
		let original = Layout::read(source)?;
		let names = original.name_guard();
		if additions
			.iter()
			.any(|addition| names.contains(addition.name))
		{
			return Err(V1AdditionIssue::ExistingMember.into());
		}
		if original
			.tables
			.hash
			.len()
			.saturating_sub(original.tables.block.len())
			< additions.len()
		{
			return Err(V1AdditionIssue::Capacity.into());
		}
		Ok(Self {
			source,
			additions,
			original,
		})
	}

	/// Exact source bytes that passed preflight.
	#[must_use]
	pub fn source(&self) -> &'a [u8] {
		self.source
	}

	/// Exact members that passed preflight.
	#[must_use]
	pub fn additions(&self) -> &'a [V1Addition<'a>] {
		self.additions
	}

	/// Proves that the candidate contains precisely the requested additive change.
	///
	/// # Errors
	///
	/// Rejects invalid output, changed retained hash slots or block metadata,
	/// changed packed bytes, unrelated changes, or mismatching new members.
	pub fn verify(&self, output: &[u8]) -> Result<(), V1AppendError> {
		let candidate = Layout::read(output)?;
		let before = &self.original;
		if before.sector_bytes != candidate.sector_bytes
			|| before.tables.hash.len() != candidate.tables.hash.len()
		{
			return Err(V1PreservationIssue::Geometry.into());
		}
		if candidate.tables.block.len() != before.tables.block.len() + self.additions.len()
			|| !candidate.tables.block.starts_with(&before.tables.block)
		{
			return Err(V1PreservationIssue::RetainedTable.into());
		}
		if candidate.payload.end < before.payload.end
			|| output.get(before.payload.clone()) != self.source.get(before.payload.clone())
		{
			return Err(V1PreservationIssue::RetainedPayload.into());
		}
		verify_additions(before, &candidate, output, self.additions)
	}
}

fn verify_additions(
	before: &Layout,
	candidate: &Layout,
	output: &[u8],
	additions: &[V1Addition<'_>],
) -> Result<(), V1AppendError> {
	let mut consumed_hash_slots = BTreeSet::new();
	for addition in additions {
		let location = lookup_file_name(
			&candidate.tables.hash,
			candidate.tables.block.len(),
			addition.name,
		)
		.ok_or(V1PreservationIssue::AddedMember)?;
		let slot = candidate.tables.hash[location.hash_index];
		let block = &candidate.tables.block[location.block_index];
		let range = packed_range(block);
		if slot.locale != 0
			|| slot.platform != 0
			|| location.block_index < before.tables.block.len()
			|| range.start < before.payload.end
			|| block.flags != MpqFileFlags::EXISTS
			|| output.get(range) != Some(addition.bytes)
			|| !consumed_hash_slots.insert(location.hash_index)
		{
			return Err(V1PreservationIssue::AddedMember.into());
		}
	}

	for (index, (old, new)) in before
		.tables
		.hash
		.iter()
		.zip(&candidate.tables.hash)
		.enumerate()
	{
		if consumed_hash_slots.contains(&index) {
			if is_live(old) {
				return Err(V1PreservationIssue::RetainedTable.into());
			}
		} else if old != new {
			return Err(V1PreservationIssue::RetainedTable.into());
		}
	}
	Ok(())
}

fn validate_additions(
	source_bytes: usize,
	additions: &[V1Addition<'_>],
) -> Result<(), V1AppendError> {
	if additions.is_empty() || additions.len() > MAX_COMPONENTS {
		return Err(V1AdditionIssue::Count.into());
	}
	let mut names = BTreeSet::new();
	let mut added_bytes = 0_usize;
	for addition in additions {
		if addition.bytes.is_empty() || addition.bytes.len() > MAX_COMPONENT_BYTES {
			return Err(V1AdditionIssue::ComponentSize.into());
		}
		added_bytes += addition.bytes.len();
		if added_bytes > MAX_TOTAL_COMPONENT_BYTES {
			return Err(V1AdditionIssue::ComponentSize.into());
		}
		if !valid_member_name(addition.name) || !names.insert(addition.name.to_ascii_lowercase()) {
			return Err(V1AdditionIssue::Name.into());
		}
	}
	let output_bytes = source_bytes
		.saturating_add(added_bytes)
		.saturating_add(additions.len() * BlockTableEntry::SERIALIZED_LEN);
	if output_bytes as u64 > V1_APPEND_ARCHIVE_LIMIT {
		return Err(V1AdditionIssue::ArchiveSize.into());
	}
	Ok(())
}

fn valid_member_name(name: &str) -> bool {
	name.len() <= MAX_MEMBER_NAME_BYTES
		&& name.split('\\').all(valid_component)
		&& !is_pseudo_name(name)
}

fn valid_component(component: &str) -> bool {
	if component.is_empty()
		|| component.starts_with(['.', ' '])
		|| component.ends_with(['.', ' '])
		|| !component
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b' ' | b'.'))
	{
		return false;
	}
	let stem = component
		.split('.')
		.next()
		.unwrap_or_default()
		.to_ascii_uppercase();
	!matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") && !port_device(&stem)
}

fn is_pseudo_name(name: &str) -> bool {
	// StormLib interprets File plus eight decimal digits and a dot as a block index.
	const PREFIX: &[u8] = b"File";
	const DIGITS: usize = 8;
	name.split_once('.')
		.and_then(|(stem, _)| stem.as_bytes().split_at_checked(PREFIX.len()))
		.is_some_and(|(prefix, digits)| {
			prefix.eq_ignore_ascii_case(PREFIX)
				&& digits.len() == DIGITS
				&& digits.iter().all(u8::is_ascii_digit)
		})
}

fn port_device(stem: &str) -> bool {
	stem.strip_prefix("COM")
		.or_else(|| stem.strip_prefix("LPT"))
		.is_some_and(|index| {
			index.len() == 1 && index.bytes().all(|digit| (b'1'..=b'9').contains(&digit))
		})
}
