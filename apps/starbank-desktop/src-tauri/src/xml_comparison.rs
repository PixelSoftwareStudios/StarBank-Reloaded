use serde::Serialize;

const BEFORE_DIFFERENCE_BYTES: usize = 2_048;
pub(crate) const EXCERPT_BYTES: usize = 16_384;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Excerpt {
	pub(crate) text: String,
	pub(crate) omits_start: bool,
	pub(crate) omits_end: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XmlComparison {
	pub(crate) first_difference_line: Option<usize>,
	pub(crate) start_line: usize,
	pub(crate) before: Excerpt,
	pub(crate) after: Excerpt,
}

impl XmlComparison {
	pub(crate) fn between(before: &str, after: &str) -> Self {
		let common_prefix = before
			.bytes()
			.zip(after.bytes())
			.take_while(|(before, after)| before == after)
			.count();
		let first_difference = before.floor_char_boundary(common_prefix);
		let (first_difference_line, start) = if common_prefix == before.len()
			&& common_prefix == after.len()
		{
			(None, 0)
		} else {
			(
				Some(line_at(before, first_difference)),
				before
					.floor_char_boundary(first_difference.saturating_sub(BEFORE_DIFFERENCE_BYTES)),
			)
		};
		XmlComparison {
			first_difference_line,
			start_line: line_at(before, start),
			before: excerpt(before, start),
			after: excerpt(after, start),
		}
	}
}

fn line_at(text: &str, index: usize) -> usize {
	let bytes = text.as_bytes();
	bytes[..index]
		.iter()
		.enumerate()
		.filter(|(offset, byte)| {
			**byte == b'\n' || (**byte == b'\r' && bytes.get(offset + 1) != Some(&b'\n'))
		})
		.count()
		+ 1
}

fn excerpt(text: &str, start: usize) -> Excerpt {
	let end = text.floor_char_boundary(text.len().min(start + EXCERPT_BYTES));
	Excerpt {
		text: text[start..end].to_owned(),
		omits_start: start > 0,
		omits_end: end < text.len(),
	}
}
