use std::{path::Path, str::FromStr};

use sc2bank::{BankDocument, Limits, ValueNode, parse};
use serde::Serialize;
use sha2::{Digest, Sha256};
use starbank_paths::parse_signing_context;
use starbank_signature::{Signature, SignatureStatus, SigningContext, verify_signature};

use crate::{
	args::ContextArgs,
	failure::{ErrorCode, Failure, Status},
	input,
};

pub(crate) const SHA256_BYTES: usize = 32;

pub(crate) fn read_bank(path: &Path) -> Result<BankDocument, Failure> {
	let limits = Limits::default();
	let bytes = input::read(path, limits.document_bytes)?;
	Ok(parse(bytes.bytes(), limits)?)
}

pub(crate) fn signing_context(
	path: &Path,
	args: ContextArgs,
) -> Result<Option<SigningContext>, Failure> {
	let supplied = match (args.author, args.player, args.bank_name) {
		(Some(author), Some(player), Some(name)) => Some((author, player, name)),
		_ => parse_signing_context(&std::path::absolute(path)?)
			.map(|hint| (hint.author, hint.player, hint.bank_name)),
	};
	supplied
		.map(|(author, player, name)| {
			SigningContext::new(author, player, name).map_err(|_| invalid_context())
		})
		.transpose()
}

pub(crate) fn invalid_context() -> Failure {
	Failure::new(
		Status::Validation,
		ErrorCode::InvalidContext,
		"A valid map author, player and bank name are required.",
	)
}

pub(crate) fn fingerprint(bytes: &[u8]) -> String {
	hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Serialize)]
pub(crate) struct BankSummary {
	pub(crate) version: Option<String>,
	pub(crate) sha256: String,
	pub(crate) sections: usize,
	pub(crate) keys: usize,
	pub(crate) values: usize,
	pub(crate) has_unprojected_nodes: bool,
	pub(crate) signature: SignatureReport,
}

impl BankSummary {
	pub(crate) fn new(document: &BankDocument, context: Option<&SigningContext>) -> Self {
		let sections = document.sections();
		let keys = sections.iter().flat_map(|section| section.keys());
		Self {
			version: document.version().map(str::to_owned),
			sha256: fingerprint(document.source()),
			sections: sections.len(),
			keys: keys.clone().count(),
			values: keys.map(|key| key.values().len()).sum(),
			has_unprojected_nodes: document.has_unprojected_bank_nodes(),
			signature: SignatureReport::new(document, context),
		}
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct SignatureReport {
	pub(crate) status: SignatureState,
	pub(crate) digest: Option<String>,
}

impl SignatureReport {
	fn new(document: &BankDocument, context: Option<&SigningContext>) -> Self {
		let status = SignatureState::from(verify_signature(document, context));
		let digest = document
			.signature()
			.and_then(|stored| stored.value())
			.and_then(|value| Signature::from_str(value).ok())
			.map(|digest| digest.to_string());
		Self { status, digest }
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SignatureState {
	Unsigned,
	Malformed,
	MissingContext,
	Unsupported,
	Valid,
	Invalid,
}

impl From<SignatureStatus> for SignatureState {
	fn from(status: SignatureStatus) -> Self {
		match status {
			SignatureStatus::Unsigned => Self::Unsigned,
			SignatureStatus::Malformed(_) => Self::Malformed,
			SignatureStatus::MissingContext => Self::MissingContext,
			SignatureStatus::Valid => Self::Valid,
			SignatureStatus::Invalid => Self::Invalid,
			_ => Self::Unsupported,
		}
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct Inspection<'bank> {
	#[serde(flatten)]
	pub(crate) summary: BankSummary,
	pub(crate) contents: Vec<SectionView<'bank>>,
}

impl<'bank> Inspection<'bank> {
	pub(crate) fn new(document: &'bank BankDocument, context: Option<&SigningContext>) -> Self {
		Self {
			summary: BankSummary::new(document, context),
			contents: document
				.sections()
				.iter()
				.map(|section| SectionView {
					name: section.name(),
					keys: section
						.keys()
						.iter()
						.map(|key| KeyView {
							name: key.name(),
							values: key.values().iter().map(ValueView::new).collect(),
						})
						.collect(),
				})
				.collect(),
		}
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct SectionView<'bank> {
	pub(crate) name: Option<&'bank str>,
	pub(crate) keys: Vec<KeyView<'bank>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct KeyView<'bank> {
	pub(crate) name: Option<&'bank str>,
	pub(crate) values: Vec<ValueView<'bank>>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ValueView<'bank> {
	pub(crate) name: &'bank str,
	pub(crate) namespace: Option<&'bank str>,
	pub(crate) kind: Option<&'bank str>,
	pub(crate) value: Option<&'bank str>,
	pub(crate) opaque: bool,
}

impl<'bank> ValueView<'bank> {
	pub(crate) fn new(node: &'bank ValueNode) -> Self {
		let attribute = node.attribute();
		Self {
			name: node.name(),
			namespace: node.namespace(),
			kind: attribute.map(|value| value.kind().as_str()),
			value: attribute.map(|value| value.value()),
			opaque: node.opaque_reason().is_some(),
		}
	}
}
