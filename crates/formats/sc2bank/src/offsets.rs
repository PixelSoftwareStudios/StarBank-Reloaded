use std::ops::Range;

use crate::{XmlEncoding, encoding::UTF16_UNIT_BYTES};

const CHECKPOINT_SPACING: usize = 128;

#[derive(Clone, Copy, Debug)]
struct Checkpoint {
	text: usize,
	native: usize,
}

/// Maps scalar boundaries without allocating a separate offset for each character.
#[derive(Clone, Debug)]
pub(crate) struct Utf16Offsets {
	checkpoints: Vec<Checkpoint>,
}

impl Utf16Offsets {
	pub(crate) fn new(text: &str) -> Self {
		let mut checkpoints = vec![Checkpoint { text: 0, native: 0 }];
		let mut latest = 0;
		let mut native = 0;
		for (offset, character) in text.char_indices() {
			if offset - latest >= CHECKPOINT_SPACING {
				checkpoints.push(Checkpoint {
					text: offset,
					native,
				});
				latest = offset;
			}
			native += character.len_utf16() * UTF16_UNIT_BYTES;
		}
		Self { checkpoints }
	}

	pub(crate) fn native_range(&self, text: &str, range: Range<usize>) -> Option<Range<usize>> {
		Some(self.native_byte(text, range.start)?..self.native_byte(text, range.end)?)
	}

	pub(crate) fn native_byte(&self, text: &str, byte: usize) -> Option<usize> {
		let index = self
			.checkpoints
			.partition_point(|point| point.text <= byte)
			.checked_sub(1)?;
		let point = self.checkpoints[index];
		let offset = text.get(point.text..byte)?.encode_utf16().count() * UTF16_UNIT_BYTES;
		Some(point.native + offset)
	}

	pub(crate) fn text_byte(&self, text: &str, byte: usize) -> Option<usize> {
		let index = self
			.checkpoints
			.partition_point(|point| point.native <= byte)
			.checked_sub(1)?;
		let point = self.checkpoints[index];
		let mut native = point.native;
		for (offset, character) in text.get(point.text..)?.char_indices() {
			if native == byte {
				return Some(point.text + offset);
			}
			native += character.len_utf16() * UTF16_UNIT_BYTES;
			if native > byte {
				return None;
			}
		}
		(native == byte).then_some(text.len())
	}
}

#[derive(Clone, Debug)]
pub(crate) struct NativeXml {
	pub encoding: XmlEncoding,
	pub bytes: Vec<u8>,
	pub offsets: Utf16Offsets,
}
