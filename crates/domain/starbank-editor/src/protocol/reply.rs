use std::fmt;

use serde::Serialize;
use starbank_core::Revision;

use super::PROTOCOL_VERSION;
use super::diff::WireDiff;
use super::error::ProtocolError;
use super::profile::{ProfilePreview, ProfileView};
use super::snapshot::{Snapshot, WireSignatureStatus};

/// Encoded reply and an optional binary payload carried outside JSON.
pub struct WireReply {
	/// JSON control reply, encoded as UTF-8 text.
	pub json: String,
	/// Raw or prepared bank bytes, present only for a successful binary response.
	pub bytes: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Scope<'a> {
	pub(super) document_id: Option<&'a str>,
	pub(super) revision: Option<Revision>,
}

#[derive(Serialize)]
struct Response<'a> {
	schema_version: u32,
	request_id: Option<&'a str>,
	document_id: Option<&'a str>,
	revision: Option<String>,
	result: ReplyResult<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
pub(super) enum ReplyResult<'a> {
	Snapshot(Snapshot<'a>),
	Profile(ProfileView<'a>),
	ProfilePreview(ProfilePreview),
	Diff(WireDiff<'a>),
	Raw {},
	Prepared {
		signature_status: WireSignatureStatus,
	},
	Closed {},
	Error {
		code: ProtocolError,
		message: &'static str,
	},
}

impl WireReply {
	pub(super) fn error(request_id: Option<&str>, scope: Scope<'_>, code: ProtocolError) -> Self {
		Self::encode(
			request_id,
			scope,
			ReplyResult::Error {
				code,
				message: code.message(),
			},
			None,
		)
	}

	pub(super) fn encode(
		request_id: Option<&str>,
		scope: Scope<'_>,
		result: ReplyResult<'_>,
		bytes: Option<Vec<u8>>,
	) -> Self {
		let serialized = serde_json::to_string(&Response {
			schema_version: PROTOCOL_VERSION,
			request_id,
			document_id: scope.document_id,
			revision: scope.revision.map(|revision| revision.to_string()),
			result,
		});
		match serialized {
			Ok(json) => Self { json, bytes },
			Err(_) => Self {
				json: SERIALIZATION_FAILURE.into(),
				bytes: None,
			},
		}
	}
}

// The built-in wire types use only infallible JSON primitives and no custom serializers.
const SERIALIZATION_FAILURE: &str = concat!(
	r#"{"schema_version":1,"request_id":null,"document_id":null,"revision":null,"result":"#,
	r#"{"kind":"error","payload":{"code":"internal","message":"The engine could not encode the response."}}}"#,
);

impl fmt::Debug for WireReply {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter
			.debug_struct("WireReply")
			.field("json_bytes", &self.json.len())
			.field("attachment_bytes", &self.bytes.as_ref().map(Vec::len))
			.finish()
	}
}
