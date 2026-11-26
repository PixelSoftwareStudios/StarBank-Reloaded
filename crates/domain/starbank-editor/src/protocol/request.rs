use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use starbank_core::Revision;

use super::diff::WireOccurrenceKind;
use super::error::ProtocolError;
use super::profile::ProfileOperation;
use super::structure::WireStructure;
use super::{MAX_OPERATIONS, PROTOCOL_VERSION};

const MAX_REQUEST_ID_BYTES: usize = 128;

pub(super) struct Request {
	pub(super) id: String,
	pub(super) action: Action,
}

pub(super) enum Action {
	Open(OpenOptions),
	InSession(Target, Operation),
}

pub(super) struct Target {
	pub(super) document_id: String,
	pub(super) expected: Revision,
}

pub(super) enum Operation {
	Snapshot,
	Profile(ProfileOperation),
	Raw,
	RawText,
	OpeningRaw,
	SetValues(Vec<WireEdit>),
	Structure(WireStructure),
	Diff,
	Revert(RevertOptions),
	ReplaceRaw,
	ReplaceRawText,
	Undo,
	Redo,
	Reset,
	Prepare(ContextOptions),
	Close(CloseOptions),
}

pub(super) struct OpenOptions {
	pub(super) context: ContextOptions,
	pub(super) discard_current: bool,
}

#[derive(Default)]
pub(super) struct ContextOptions {
	pub(super) context: Option<WireContext>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CloseOptions {
	pub(super) discard: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevertOptions {
	pub(super) kind: WireOccurrenceKind,
	pub(super) id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireContext {
	pub(super) author: String,
	pub(super) player: String,
	pub(super) bank_name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireEdit {
	pub(super) id: String,
	pub(super) value: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOpenOptions<'a> {
	#[serde(borrow, default, deserialize_with = "present")]
	context: Option<&'a RawValue>,
	#[serde(default)]
	discard_current: bool,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContextOptions<'a> {
	#[serde(borrow, default, deserialize_with = "present")]
	context: Option<&'a RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetValuesOptions<'a> {
	#[serde(borrow)]
	edits: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<'a> {
	schema_version: u32,
	request_id: String,
	#[serde(default, deserialize_with = "present")]
	document_id: Option<String>,
	#[serde(default, deserialize_with = "present")]
	expected_revision: Option<String>,
	#[serde(borrow)]
	command: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand<'a> {
	kind: CommandKind,
	#[serde(borrow, default, deserialize_with = "present")]
	payload: Option<&'a RawValue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommandKind {
	Open,
	ProfileLoad,
	ProfileSnapshot,
	ProfileClear,
	ProfilePreview,
	ProfileApply,
	Snapshot,
	Raw,
	RawText,
	OpeningRaw,
	SetValues,
	Structure,
	Diff,
	Revert,
	ReplaceRaw,
	ReplaceRawText,
	Undo,
	Redo,
	Reset,
	Prepare,
	Close,
}

#[derive(Deserialize)]
struct Correlation {
	request_id: String,
}

pub(super) fn decode(control: &str) -> Result<Request, (Option<String>, ProtocolError)> {
	if !control
		.trim_start_matches([' ', '\t', '\r', '\n'])
		.starts_with('{')
	{
		return Err((None, ProtocolError::InvalidRequest));
	}
	let envelope = serde_json::from_str::<Envelope<'_>>(control).map_err(|_| {
		let id = serde_json::from_str::<Correlation>(control)
			.ok()
			.map(|header| header.request_id)
			.filter(|id| valid_request_id(id));
		(id, ProtocolError::InvalidRequest)
	})?;
	if !valid_request_id(&envelope.request_id) {
		return Err((None, ProtocolError::InvalidRequest));
	}
	let action = decode_action(
		envelope.schema_version,
		envelope.document_id,
		envelope.expected_revision,
		envelope.command,
	);
	match action {
		Ok(action) => Ok(Request {
			id: envelope.request_id,
			action,
		}),
		Err(error) => Err((Some(envelope.request_id), error)),
	}
}

fn valid_request_id(id: &str) -> bool {
	!id.is_empty() && id.len() <= MAX_REQUEST_ID_BYTES && !id.chars().any(char::is_control)
}

fn decode_action(
	version: u32,
	document_id: Option<String>,
	expected: Option<String>,
	command: &RawValue,
) -> Result<Action, ProtocolError> {
	if version != PROTOCOL_VERSION {
		return Err(ProtocolError::UnsupportedVersion);
	}
	let RawCommand { kind, payload } = decode_payload(command)?;
	if matches!(kind, CommandKind::Open) {
		if document_id.is_some() || expected.is_some() {
			return Err(ProtocolError::InvalidRequest);
		}
		let options: RawOpenOptions<'_> = optional_payload(payload)?;
		return Ok(Action::Open(OpenOptions {
			context: ContextOptions {
				context: options.context.map(decode_payload).transpose()?,
			},
			discard_current: options.discard_current,
		}));
	}

	let target = Target {
		document_id: document_id.ok_or(ProtocolError::InvalidRequest)?,
		expected: expected
			.ok_or(ProtocolError::InvalidRequest)?
			.parse()
			.map_err(|_| ProtocolError::InvalidRevision)?,
	};
	let operation = match kind {
		CommandKind::Open => return Err(ProtocolError::InvalidRequest),
		CommandKind::SetValues => {
			let SetValuesOptions { edits } = required_payload(payload)?;
			if edits.is_empty() {
				return Err(ProtocolError::EmptyEdit);
			}
			if edits.len() > MAX_OPERATIONS {
				return Err(ProtocolError::TooManyEdits);
			}
			Operation::SetValues(
				edits
					.into_iter()
					.map(decode_payload)
					.collect::<Result<_, _>>()?,
			)
		}
		CommandKind::Structure => Operation::Structure(required_payload(payload)?),
		CommandKind::Revert => Operation::Revert(required_payload(payload)?),
		CommandKind::ProfileLoad => {
			Operation::Profile(ProfileOperation::Load(required_payload(payload)?))
		}
		CommandKind::ProfilePreview => {
			Operation::Profile(ProfileOperation::Preview(required_payload(payload)?))
		}
		CommandKind::ProfileApply => {
			Operation::Profile(ProfileOperation::Apply(required_payload(payload)?))
		}
		CommandKind::Prepare => Operation::Prepare(context_options(payload)?),
		CommandKind::Close => Operation::Close(optional_payload(payload)?),
		kind => {
			require_absent(payload)?;
			payload_free_operation(kind)?
		}
	};
	Ok(Action::InSession(target, operation))
}

fn payload_free_operation(kind: CommandKind) -> Result<Operation, ProtocolError> {
	match kind {
		CommandKind::Snapshot => Ok(Operation::Snapshot),
		CommandKind::ProfileSnapshot => Ok(Operation::Profile(ProfileOperation::Snapshot)),
		CommandKind::ProfileClear => Ok(Operation::Profile(ProfileOperation::Clear)),
		CommandKind::Raw => Ok(Operation::Raw),
		CommandKind::RawText => Ok(Operation::RawText),
		CommandKind::OpeningRaw => Ok(Operation::OpeningRaw),
		CommandKind::Diff => Ok(Operation::Diff),
		CommandKind::ReplaceRaw => Ok(Operation::ReplaceRaw),
		CommandKind::ReplaceRawText => Ok(Operation::ReplaceRawText),
		CommandKind::Undo => Ok(Operation::Undo),
		CommandKind::Redo => Ok(Operation::Redo),
		CommandKind::Reset => Ok(Operation::Reset),
		_ => Err(ProtocolError::InvalidRequest),
	}
}

fn require_absent(payload: Option<&RawValue>) -> Result<(), ProtocolError> {
	if payload.is_some() {
		return Err(ProtocolError::InvalidRequest);
	}
	Ok(())
}

fn context_options(payload: Option<&RawValue>) -> Result<ContextOptions, ProtocolError> {
	let options: RawContextOptions<'_> = optional_payload(payload)?;
	Ok(ContextOptions {
		context: options.context.map(decode_payload).transpose()?,
	})
}

fn required_payload<'a, T: Deserialize<'a>>(
	payload: Option<&'a RawValue>,
) -> Result<T, ProtocolError> {
	decode_payload(payload.ok_or(ProtocolError::InvalidRequest)?)
}

fn optional_payload<'a, T: Default + Deserialize<'a>>(
	payload: Option<&'a RawValue>,
) -> Result<T, ProtocolError> {
	payload.map_or_else(|| Ok(T::default()), decode_payload)
}

fn decode_payload<'a, T: Deserialize<'a>>(payload: &'a RawValue) -> Result<T, ProtocolError> {
	let text = payload.get();
	if !text.starts_with('{') {
		return Err(ProtocolError::InvalidRequest);
	}
	serde_json::from_str(text).map_err(|_| ProtocolError::InvalidRequest)
}

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
	deserializer: D,
) -> Result<Option<T>, D::Error> {
	T::deserialize(deserializer).map(Some)
}
