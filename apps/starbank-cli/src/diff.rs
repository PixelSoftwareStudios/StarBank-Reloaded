use std::ops::Range;

use sc2bank::{BankDocument, Key, Section};
use serde::Serialize;

use crate::bank::{ValueView, fingerprint};

#[derive(Debug, Serialize)]
pub(crate) struct BankDiff<'bank> {
	pub(crate) source_changed: bool,
	pub(crate) before_sha256: String,
	pub(crate) after_sha256: String,
	pub(crate) version: Change<Option<&'bank str>>,
	pub(crate) stored_signature: Change<Option<&'bank str>>,
	pub(crate) sections: Vec<SectionChange<'bank>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Change<T> {
	before: T,
	after: T,
}

impl<T: PartialEq> Change<T> {
	fn changed(&self) -> bool {
		self.before != self.after
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct SectionChange<'bank> {
	index: usize,
	presence: Change<bool>,
	name: Change<Option<&'bank str>>,
	keys: Vec<KeyChange<'bank>>,
}

#[derive(Debug, Serialize)]
struct KeyChange<'bank> {
	index: usize,
	presence: Change<bool>,
	name: Change<Option<&'bank str>>,
	values: Vec<ValueChange<'bank>>,
}

#[derive(Debug, Serialize)]
struct ValueChange<'bank> {
	index: usize,
	#[serde(flatten)]
	change: Change<Option<ValueView<'bank>>>,
}

impl<'bank> BankDiff<'bank> {
	pub(crate) fn new(before: &'bank BankDocument, after: &'bank BankDocument) -> Self {
		let source_changed = before.source() != after.source();
		let before_sha256 = fingerprint(before.source());
		let after_sha256 = if source_changed {
			fingerprint(after.source())
		} else {
			before_sha256.clone()
		};
		Self {
			source_changed,
			before_sha256,
			after_sha256,
			version: Change {
				before: before.version(),
				after: after.version(),
			},
			stored_signature: Change {
				before: before.signature().and_then(|signature| signature.value()),
				after: after.signature().and_then(|signature| signature.value()),
			},
			sections: indices(before.sections(), after.sections())
				.filter_map(|index| {
					section_changes(
						index,
						before.sections().get(index),
						after.sections().get(index),
					)
				})
				.collect(),
		}
	}
}

fn section_changes<'bank>(
	index: usize,
	before: Option<&'bank Section>,
	after: Option<&'bank Section>,
) -> Option<SectionChange<'bank>> {
	let before_keys = before.map_or(&[][..], Section::keys);
	let after_keys = after.map_or(&[][..], Section::keys);
	let keys = indices(before_keys, after_keys)
		.filter_map(|index| key_changes(index, before_keys.get(index), after_keys.get(index)))
		.collect::<Vec<_>>();
	let presence = Change {
		before: before.is_some(),
		after: after.is_some(),
	};
	let name = Change {
		before: before.and_then(Section::name),
		after: after.and_then(Section::name),
	};
	(presence.changed() || name.changed() || !keys.is_empty()).then_some(SectionChange {
		index,
		presence,
		name,
		keys,
	})
}

fn key_changes<'bank>(
	index: usize,
	before: Option<&'bank Key>,
	after: Option<&'bank Key>,
) -> Option<KeyChange<'bank>> {
	let before_values = before.map_or(&[][..], Key::values);
	let after_values = after.map_or(&[][..], Key::values);
	let values = indices(before_values, after_values)
		.filter_map(|index| {
			let change = Change {
				before: before_values.get(index).map(ValueView::new),
				after: after_values.get(index).map(ValueView::new),
			};
			change.changed().then_some(ValueChange { index, change })
		})
		.collect::<Vec<_>>();
	let presence = Change {
		before: before.is_some(),
		after: after.is_some(),
	};
	let name = Change {
		before: before.and_then(Key::name),
		after: after.and_then(Key::name),
	};
	(presence.changed() || name.changed() || !values.is_empty()).then_some(KeyChange {
		index,
		presence,
		name,
		values,
	})
}

fn indices<T>(before: &[T], after: &[T]) -> Range<usize> {
	0..before.len().max(after.len())
}
