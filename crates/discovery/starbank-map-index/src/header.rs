use crate::metadata::display_text;

const TITLE_MARKER: &[u8] = b"DocInfo/Name";
const MARKER_ENVELOPE_BYTES: usize = 6;
const TRAILING_FIELD_BYTES: usize = 1;

/// Extract only the legacy title record. Do not infer unknown binary fields.
pub(crate) fn title(header: &[u8]) -> Option<&str> {
	let mut values = header
		.windows(TITLE_MARKER.len())
		.enumerate()
		.filter(|(_, marker)| *marker == TITLE_MARKER)
		.filter_map(|(offset, _)| {
			let tail = header.get(offset + TITLE_MARKER.len()..)?;
			let (envelope, content) = tail.split_at_checked(MARKER_ENVELOPE_BYTES)?;
			if envelope.last() != Some(&0) {
				return None;
			}
			let end = content
				.iter()
				.take(crate::metadata::MAX_DISPLAY_BYTES + TRAILING_FIELD_BYTES + 1)
				.position(|byte| *byte == 0)?;
			let value = content.get(..end.checked_sub(TRAILING_FIELD_BYTES)?)?;
			display_text(str::from_utf8(value).ok()?)
		});
	let value = values.next()?;
	// Diverging repeated records make the selected locale ambiguous.
	values.all(|candidate| candidate == value).then_some(value)
}
