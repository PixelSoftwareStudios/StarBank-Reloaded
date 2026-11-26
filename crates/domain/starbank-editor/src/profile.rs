use std::{collections::BTreeMap, error::Error, fmt};

use sc2bank::{ValueEdit, serialize};
use starbank_core::Revision;
use starbank_profiles::{
	BankIdentity, Control, MatchOutcome, ProfileEditError, ProfileInput, ProfileRegistry,
	ProposedValueEdit,
};

use crate::{EditorError, OccurrenceId, Session, normalize_value};

/// Optional map evidence obtained by the application from a trusted local source.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileEvidence<'a> {
	/// Verified map version, if known.
	pub map_version: Option<&'a str>,
	/// Verified map fingerprint, if known.
	pub fingerprint: Option<&'a str>,
}

/// A request to edit one control in the identified profile.
#[derive(Clone, Copy, Debug)]
pub struct ProfileEdit<'a> {
	/// ID of the visible profile in the caller's checked projection.
	pub profile_id: &'a str,
	/// ID of the control the user intends to change.
	pub field_id: &'a str,
	/// The user's exact input, including numeric grouping when present.
	pub input: ProfileInput<'a>,
	/// Version or fingerprint known to the caller from local map metadata.
	pub evidence: ProfileEvidence<'a>,
}

/// One value change in a profile preview, addressed by stable editor ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileValueChange {
	/// Identifier of the friendly field.
	pub field_id: String,
	/// Stable value occurrence in the open session.
	pub id: OccurrenceId,
	/// Exact previous value for the preview.
	pub previous: String,
	/// Exact replacement value for the preview.
	pub value: String,
}

/// Why a checked profile request cannot edit the current bank.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProfileSessionError {
	/// No session identity, or no profile for that identity.
	Unknown,
	/// A known identity's profile could not bind to the current bank.
	Incompatible,
	/// Several profiles qualified with equal confidence.
	Ambiguous,
	/// The current profile differs from the one requested.
	ProfileChanged,
	/// An occurrence or previous value differs from the bound proposal.
	ChangedValue,
	/// The selected field or a dependent field rejected the requested value.
	Edit(ProfileEditError),
}

impl fmt::Display for ProfileSessionError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Unknown => formatter.write_str("the current bank has no known profile"),
			Self::Incompatible => {
				formatter.write_str("the current bank does not match its profile")
			}
			Self::Ambiguous => {
				formatter.write_str("more than one profile matches the current bank")
			}
			Self::ProfileChanged => formatter.write_str("the current profile changed"),
			Self::ChangedValue => {
				formatter.write_str("a value no longer matches the profile binding")
			}
			Self::Edit(source) => source.fmt(formatter),
		}
	}
}

impl Error for ProfileSessionError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		if let Self::Edit(source) = self {
			Some(source)
		} else {
			None
		}
	}
}

impl Session {
	/// Resolves a profile using the open bank's identity and optional local map evidence.
	#[must_use]
	pub fn match_profile<'a>(
		&'a self,
		registry: &'a ProfileRegistry,
		evidence: ProfileEvidence<'_>,
	) -> MatchOutcome<'a> {
		let Some(context) = self.context.as_ref() else {
			return MatchOutcome::Unknown;
		};
		registry.match_bank(
			&self.document,
			&BankIdentity {
				author: Some(context.author()),
				bank_name: context.bank_name(),
				map_version: evidence.map_version,
				fingerprint: evidence.fingerprint,
			},
		)
	}

	/// Previews all changes to the active bank without modifying the session.
	///
	/// # Errors
	///
	/// A stale revision, incompatible profile or rejected edit produces no partial preview.
	pub fn preview_profile_edit(
		&self,
		expected: Revision,
		registry: &ProfileRegistry,
		edit: ProfileEdit<'_>,
	) -> Result<Vec<ProfileValueChange>, EditorError> {
		let proposal = self.checked_profile_edits(expected, registry, edit)?;
		if !proposal.is_empty() {
			serialize(&self.document, &value_edits(&proposal), self.limits)
				.map_err(EditorError::Serialize)?;
		}
		proposal
			.into_iter()
			.map(|change| {
				let id = self
					.id_of_value(change.target)
					.ok_or(EditorError::Profile(ProfileSessionError::ChangedValue))?;
				Ok(ProfileValueChange {
					field_id: change.field_id,
					id,
					previous: change.previous,
					value: change.value,
				})
			})
			.collect()
	}

	/// Applies one qualified profile input and all dependent changes as one undo step.
	///
	/// # Errors
	///
	/// Rejects stale revisions, invalid bindings, unsafe output or exceeded document limits.
	/// The confirmed document remains intact when any part fails.
	pub fn apply_profile_edit(
		&mut self,
		expected: Revision,
		registry: &ProfileRegistry,
		edit: ProfileEdit<'_>,
	) -> Result<Revision, EditorError> {
		let proposal = self.checked_profile_edits(expected, registry, edit)?;
		self.commit_value_edits(&value_edits(&proposal))
	}

	fn checked_profile_edits(
		&self,
		expected: Revision,
		registry: &ProfileRegistry,
		edit: ProfileEdit<'_>,
	) -> Result<Vec<ProposedValueEdit>, EditorError> {
		self.require_revision(expected)?;
		let binding = match self.match_profile(registry, edit.evidence) {
			MatchOutcome::Matched(binding) => binding,
			MatchOutcome::Incompatible => {
				return Err(EditorError::Profile(ProfileSessionError::Incompatible));
			}
			MatchOutcome::Ambiguous(_) => {
				return Err(EditorError::Profile(ProfileSessionError::Ambiguous));
			}
			_ => return Err(EditorError::Profile(ProfileSessionError::Unknown)),
		};
		if binding.id() != edit.profile_id {
			return Err(EditorError::Profile(ProfileSessionError::ProfileChanged));
		}

		if let ProfileInput::Text(value) = edit.input
			&& value.len() > self.limits.scalar_bytes
		{
			return Err(EditorError::EditLimit);
		}
		let field = binding
			.groups()
			.flat_map(|group| group.fields)
			.find(|field| field.id == edit.field_id)
			.ok_or(EditorError::Profile(ProfileSessionError::Edit(
				ProfileEditError::UnknownField,
			)))?;
		if field.read_only {
			return Err(EditorError::Profile(ProfileSessionError::Edit(
				ProfileEditError::ReadOnly,
			)));
		}
		let normalized;
		let input = if let (Control::Number { .. }, ProfileInput::Text(value)) =
			(field.control, edit.input)
		{
			let id = self
				.id_of_value(field.target)
				.ok_or(EditorError::Profile(ProfileSessionError::ChangedValue))?;
			normalized = normalize_value(field.kind, value)
				.map_err(|source| EditorError::InvalidValue { id, source })?;
			ProfileInput::Text(normalized.as_ref())
		} else {
			edit.input
		};
		let edits = binding
			.propose_edit(edit.field_id, input)
			.map_err(|error| EditorError::Profile(ProfileSessionError::Edit(error)))?;
		self.verify_profile_targets(&edits)?;
		Ok(edits)
	}

	fn verify_profile_targets(&self, edits: &[ProposedValueEdit]) -> Result<(), EditorError> {
		if edits.is_empty() {
			return Ok(());
		}
		let mut remaining = BTreeMap::new();
		for edit in edits {
			if edit.value.len() > self.limits.scalar_bytes {
				return Err(EditorError::EditLimit);
			}
			if remaining
				.insert(edit.target, edit.previous.as_str())
				.is_some()
			{
				return Err(EditorError::Profile(ProfileSessionError::ChangedValue));
			}
		}
		for value in self
			.document
			.sections()
			.iter()
			.flat_map(|section| section.keys())
			.flat_map(|key| key.values())
		{
			if let Some(previous) = remaining.remove(&value.id()) {
				if value
					.attribute()
					.is_none_or(|attribute| attribute.value() != previous)
					|| self.id_of_value(value.id()).is_none()
				{
					return Err(EditorError::Profile(ProfileSessionError::ChangedValue));
				}
				if remaining.is_empty() {
					return Ok(());
				}
			}
		}
		Err(EditorError::Profile(ProfileSessionError::ChangedValue))
	}
}

fn value_edits(proposal: &[ProposedValueEdit]) -> Vec<ValueEdit<'_>> {
	proposal
		.iter()
		.map(|change| ValueEdit {
			id: change.target,
			value: &change.value,
		})
		.collect()
}
