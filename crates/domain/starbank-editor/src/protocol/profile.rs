use std::str;

use serde::{Deserialize, Serialize};
use starbank_core::{Revision, is_sc2_account_handle};
use starbank_profiles::{
	Confidence, Control, MatchOutcome, Profile, ProfileEditError, ProfileInput, ProfileRegistry,
};

use crate::{
	EditorError, ProfileEdit, ProfileEvidence, ProfileSessionError, ProfileValueChange, Session,
};

use super::{Output, error::ProtocolError, require_attachment};
use crate::profile::ProfileIdentity;

pub(super) const MAX_PROFILE_BYTES: usize = 64 * 1024;
const MAX_PROFILE_IDENTITY_BYTES: usize = 256;

#[derive(Default)]
pub(super) struct Profiles {
	registry: Option<ProfileRegistry>,
	identity: Option<WireProfileIdentity>,
	preview: Option<Pending>,
	sequence: u64,
}

struct Pending {
	id: u64,
	revision: Revision,
	edit: WireProfileEdit,
}

pub(super) enum ProfileOperation {
	Load(WireProfileLoad),
	Snapshot,
	Clear,
	Preview(WireProfileEdit),
	Apply(WireProfileApply),
}

#[derive(Deserialize)]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum WireProfileLoad {
	Json {
		identity: Option<WireProfileIdentity>,
	},
	LegacyYaml {
		profile_id: String,
		identity: Option<WireProfileIdentity>,
	},
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireProfileIdentity {
	author: String,
	bank_name: String,
}

impl WireProfileIdentity {
	fn borrow(&self) -> ProfileIdentity<'_> {
		ProfileIdentity {
			author: &self.author,
			bank_name: &self.bank_name,
		}
	}

	fn validate(&self, session: &Session) -> Result<(), ProtocolError> {
		let valid = self.author.len() <= MAX_PROFILE_IDENTITY_BYTES
			&& is_sc2_account_handle(&self.author)
			&& !self.bank_name.trim().is_empty()
			&& self.bank_name.len() <= MAX_PROFILE_IDENTITY_BYTES
			&& !matches!(self.bank_name.as_str(), "." | "..")
			&& !self.bank_name.contains(['/', '\\', '\0'])
			&& !self.bank_name.chars().any(char::is_control);
		if !valid
			|| session.context.as_ref().is_some_and(|context| {
				context.author() != self.author || context.bank_name() != self.bank_name
			}) {
			return Err(ProtocolError::InvalidProfileIdentity);
		}
		Ok(())
	}
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireProfileEdit {
	profile_id: String,
	field_id: String,
	input: WireProfileInput,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireProfileInput {
	Text { value: String },
	Checked { value: bool },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireProfileApply {
	preview_id: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum ProfileView<'a> {
	Unloaded {},
	MissingContext {},
	Unrecognized {},
	Incompatible {},
	Ambiguous {
		profile_ids: Vec<&'a str>,
	},
	Matched {
		profile_id: &'a str,
		confidence: MatchConfidence,
		groups: Vec<WireProfileGroup<'a>>,
	},
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MatchConfidence {
	Legacy,
	IdentityAndSchema,
	Version,
	Fingerprint,
	VersionAndFingerprint,
}

impl From<Confidence> for MatchConfidence {
	fn from(value: Confidence) -> Self {
		match value {
			Confidence::Legacy => Self::Legacy,
			Confidence::IdentityAndSchema => Self::IdentityAndSchema,
			Confidence::Version => Self::Version,
			Confidence::Fingerprint => Self::Fingerprint,
			Confidence::VersionAndFingerprint => Self::VersionAndFingerprint,
		}
	}
}

#[derive(Serialize)]
pub(super) struct WireProfileGroup<'a> {
	label: &'a str,
	description: Option<&'a str>,
	fields: Vec<WireProfileField<'a>>,
}

#[derive(Serialize)]
struct WireProfileField<'a> {
	id: &'a str,
	target_id: String,
	label: &'a str,
	description: Option<&'a str>,
	kind: FieldKind,
	value: &'a str,
	checked: Option<bool>,
	read_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum FieldKind {
	Number,
	Checkbox,
	Text,
}

#[derive(Serialize)]
pub(super) struct ProfilePreview {
	preview_id: String,
	changes: Vec<WireProfileChange>,
}

#[derive(Serialize)]
struct WireProfileChange {
	field_id: String,
	id: String,
	previous: String,
	value: String,
}

impl From<ProfileValueChange> for WireProfileChange {
	fn from(value: ProfileValueChange) -> Self {
		Self {
			field_id: value.field_id,
			id: value.id.to_string(),
			previous: value.previous,
			value: value.value,
		}
	}
}

impl Profiles {
	pub(super) fn view<'a>(
		&'a self,
		session: &'a Session,
	) -> Result<ProfileView<'a>, ProtocolError> {
		let Some(registry) = &self.registry else {
			return Ok(ProfileView::Unloaded {});
		};
		if session.context.is_none() && self.identity.is_none() {
			return Ok(ProfileView::MissingContext {});
		}
		match session.match_profile_using(
			registry,
			ProfileEvidence::default(),
			self.identity.as_ref().map(WireProfileIdentity::borrow),
		) {
			MatchOutcome::Incompatible => Ok(ProfileView::Incompatible {}),
			MatchOutcome::Ambiguous(profile_ids) => Ok(ProfileView::Ambiguous { profile_ids }),
			MatchOutcome::Matched(profile) => {
				let groups = profile
					.groups()
					.map(|group| {
						let fields = group
							.fields
							.iter()
							.map(|field| {
								let id = session
									.id_of_value(field.target)
									.ok_or(ProtocolError::Internal)?;
								let kind = match field.control {
									Control::Number { .. } => FieldKind::Number,
									Control::Checkbox { .. } => FieldKind::Checkbox,
									Control::Text { .. } => FieldKind::Text,
									_ => return Err(ProtocolError::Internal),
								};
								Ok(WireProfileField {
									id: field.id,
									target_id: id.to_string(),
									label: field.label,
									description: field.description,
									kind,
									value: field.lexeme,
									checked: field.checked(),
									read_only: field.read_only,
								})
							})
							.collect::<Result<_, _>>()?;
						Ok(WireProfileGroup {
							label: group.label,
							description: group.description,
							fields,
						})
					})
					.collect::<Result<_, _>>()?;
				Ok(ProfileView::Matched {
					profile_id: profile.id(),
					confidence: profile.confidence().into(),
					groups,
				})
			}
			_ => Ok(ProfileView::Unrecognized {}),
		}
	}

	pub(super) fn execute(
		&mut self,
		session: &mut Session,
		expected: Revision,
		operation: ProfileOperation,
		bytes: Option<&[u8]>,
	) -> Result<Output, ProtocolError> {
		if let ProfileOperation::Load(options) = operation {
			let attachment = require_attachment(bytes, MAX_PROFILE_BYTES)?;
			let source = str::from_utf8(attachment).map_err(|_| ProtocolError::InvalidProfile)?;
			let (legacy_id, identity) = match options {
				WireProfileLoad::Json { identity } => (None, identity),
				WireProfileLoad::LegacyYaml {
					profile_id,
					identity,
				} => (Some(profile_id), identity),
			};
			if let Some(identity) = &identity {
				identity.validate(session)?;
			}
			let profile = if let Some(profile_id) = legacy_id {
				let author = session
					.context
					.as_ref()
					.map(|context| context.author())
					.or_else(|| identity.as_ref().map(|identity| identity.author.as_str()))
					.ok_or(ProtocolError::ProfileContextRequired)?;
				Profile::import_legacy_yaml(source, &profile_id, author)
			} else {
				Profile::from_json(source)
			}
			.map_err(|_| ProtocolError::InvalidProfile)?;
			let registry =
				ProfileRegistry::new(vec![profile]).map_err(|_| ProtocolError::InvalidProfile)?;
			self.registry = Some(registry);
			self.identity = identity;
			self.preview = None;
			return Ok(Output::Profile);
		}
		if bytes.is_some() {
			return Err(ProtocolError::UnexpectedAttachment);
		}
		match operation {
			ProfileOperation::Snapshot => Ok(Output::Profile),
			ProfileOperation::Clear => {
				self.registry = None;
				self.identity = None;
				self.preview = None;
				Ok(Output::Profile)
			}
			ProfileOperation::Preview(edit) => self.preview(session, expected, edit),
			ProfileOperation::Apply(request) => self.apply(session, expected, request),
			ProfileOperation::Load(_) => Err(ProtocolError::Internal),
		}
	}

	fn preview(
		&mut self,
		session: &Session,
		expected: Revision,
		edit: WireProfileEdit,
	) -> Result<Output, ProtocolError> {
		self.preview = None;
		let registry = self
			.registry
			.as_ref()
			.ok_or(ProtocolError::ProfileUnavailable)?;
		let changes = session
			.preview_profile_using(
				expected,
				registry,
				edit.as_edit(),
				self.identity.as_ref().map(WireProfileIdentity::borrow),
			)
			.map_err(|error| profile_error(&error))?;
		let preview_id = self
			.sequence
			.checked_add(1)
			.ok_or(ProtocolError::Internal)?;
		self.sequence = preview_id;
		if !changes.is_empty() {
			self.preview = Some(Pending {
				id: preview_id,
				revision: expected,
				edit,
			});
		}
		Ok(Output::ProfilePreview(ProfilePreview {
			preview_id: preview_id.to_string(),
			changes: changes.into_iter().map(Into::into).collect(),
		}))
	}

	fn apply(
		&mut self,
		session: &mut Session,
		expected: Revision,
		request: WireProfileApply,
	) -> Result<Output, ProtocolError> {
		let token = request
			.preview_id
			.parse::<u64>()
			.ok()
			.filter(|token| token.to_string() == request.preview_id)
			.ok_or(ProtocolError::StaleProfilePreview)?;
		if self
			.preview
			.as_ref()
			.is_none_or(|preview| preview.id != token || preview.revision != expected)
		{
			return Err(ProtocolError::StaleProfilePreview);
		}
		let pending = self.preview.take().ok_or(ProtocolError::Internal)?;
		let registry = self
			.registry
			.as_ref()
			.ok_or(ProtocolError::ProfileUnavailable)?;
		let _ = session
			.apply_profile_using(
				expected,
				registry,
				pending.edit.as_edit(),
				self.identity.as_ref().map(WireProfileIdentity::borrow),
			)
			.map_err(|error| profile_error(&error))?;
		Ok(Output::Snapshot)
	}
}

impl WireProfileEdit {
	fn as_edit(&self) -> ProfileEdit<'_> {
		ProfileEdit {
			profile_id: &self.profile_id,
			field_id: &self.field_id,
			input: match &self.input {
				WireProfileInput::Text { value } => ProfileInput::Text(value),
				WireProfileInput::Checked { value } => ProfileInput::Checked(*value),
			},
			evidence: ProfileEvidence::default(),
		}
	}
}

fn profile_error(error: &EditorError) -> ProtocolError {
	match error {
		EditorError::Profile(
			ProfileSessionError::Unknown
			| ProfileSessionError::Incompatible
			| ProfileSessionError::Ambiguous
			| ProfileSessionError::ProfileChanged,
		) => ProtocolError::ProfileUnavailable,
		EditorError::Profile(ProfileSessionError::Edit(ProfileEditError::InvalidValue(_))) => {
			ProtocolError::InvalidProfileValue
		}
		EditorError::Profile(ProfileSessionError::Edit(ProfileEditError::Overflow(_))) => {
			ProtocolError::ProfileArithmeticOverflow
		}
		EditorError::Profile(ProfileSessionError::Edit(ProfileEditError::ReadOnly)) => {
			ProtocolError::ProfileReadOnly
		}
		_ => ProtocolError::editor(error, ProtocolError::InvalidEdit),
	}
}
