use serde::{Deserialize, Serialize};

use crate::{DiffOccurrence, OccurrenceChange, OccurrenceDiff, OccurrenceKind, OpeningDiff};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum WireOccurrenceKind {
	Section,
	Key,
	Value,
}

impl From<OccurrenceKind> for WireOccurrenceKind {
	fn from(kind: OccurrenceKind) -> Self {
		match kind {
			OccurrenceKind::Section => Self::Section,
			OccurrenceKind::Key => Self::Key,
			OccurrenceKind::Value => Self::Value,
		}
	}
}

#[derive(Serialize)]
pub(super) struct WireDiff<'a> {
	source_changed: bool,
	changes: Vec<WireOccurrenceDiff<'a>>,
}

impl<'a> From<&'a OpeningDiff> for WireDiff<'a> {
	fn from(diff: &'a OpeningDiff) -> Self {
		Self {
			source_changed: diff.source_changed,
			changes: diff.changes.iter().map(WireOccurrenceDiff::from).collect(),
		}
	}
}

#[derive(Serialize)]
struct WireOccurrenceDiff<'a> {
	id: String,
	kind: WireOccurrenceKind,
	change: WireOccurrenceChange<'a>,
}

impl<'a> From<&'a OccurrenceDiff> for WireOccurrenceDiff<'a> {
	fn from(diff: &'a OccurrenceDiff) -> Self {
		let change = match &diff.change {
			OccurrenceChange::Added(after) => WireOccurrenceChange::Added(after.into()),
			OccurrenceChange::Removed(before) => WireOccurrenceChange::Removed(before.into()),
			OccurrenceChange::Changed { before, after } => WireOccurrenceChange::Changed {
				before: before.into(),
				after: after.into(),
			},
		};
		Self {
			id: diff.id.to_string(),
			kind: diff.kind.into(),
			change,
		}
	}
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
enum WireOccurrenceChange<'a> {
	Added(WireDiffOccurrence<'a>),
	Removed(WireDiffOccurrence<'a>),
	Changed {
		before: WireDiffOccurrence<'a>,
		after: WireDiffOccurrence<'a>,
	},
}

#[derive(Serialize)]
struct WireDiffOccurrence<'a> {
	parent: Option<String>,
	name: Option<&'a str>,
	value: Option<WireDiffValue<'a>>,
}

impl<'a> From<&'a DiffOccurrence> for WireDiffOccurrence<'a> {
	fn from(occurrence: &'a DiffOccurrence) -> Self {
		Self {
			parent: occurrence.parent.map(|id| id.to_string()),
			name: occurrence.name.as_deref(),
			value: occurrence.value.as_ref().map(|value| WireDiffValue {
				kind: value.kind.as_str(),
				text: &value.text,
			}),
		}
	}
}

#[derive(Serialize)]
struct WireDiffValue<'a> {
	kind: &'a str,
	text: &'a str,
}
