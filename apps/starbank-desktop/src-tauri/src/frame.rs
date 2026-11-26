use starbank_editor::{MAX_CONTROL_BYTES, WireReply, max_attachment_bytes};

const FLAGS_BYTES: usize = 1;
const CONTROL_LENGTH_BYTES: usize = std::mem::size_of::<u32>();
const HEADER_BYTES: usize = FLAGS_BYTES + CONTROL_LENGTH_BYTES;
const CONTROL_ONLY: u8 = 0;
const WITH_ATTACHMENT: u8 = 1;

pub(crate) fn decode(frame: &[u8]) -> Option<(&str, Option<&[u8]>)> {
	let (header, data) = frame.split_at_checked(HEADER_BYTES)?;
	let (flag, length) = header.split_first()?;
	let length = usize::try_from(u32::from_le_bytes(length.try_into().ok()?)).ok()?;
	if length > MAX_CONTROL_BYTES {
		return None;
	}
	let (control, attachment) = data.split_at_checked(length)?;
	let attachment = match *flag {
		CONTROL_ONLY if attachment.is_empty() => None,
		WITH_ATTACHMENT if attachment.len() <= max_attachment_bytes() => Some(attachment),
		_ => return None,
	};
	Some((str::from_utf8(control).ok()?, attachment))
}

pub(crate) fn encode(reply: WireReply) -> Option<Vec<u8>> {
	let length = u32::try_from(reply.json.len()).ok()?;
	let mut frame = Vec::with_capacity(
		HEADER_BYTES
			.checked_add(reply.json.len())?
			.checked_add(reply.bytes.as_ref().map_or(0, Vec::len))?,
	);
	frame.push(if reply.bytes.is_some() {
		WITH_ATTACHMENT
	} else {
		CONTROL_ONLY
	});
	frame.extend_from_slice(&length.to_le_bytes());
	frame.extend_from_slice(reply.json.as_bytes());
	if let Some(bytes) = reply.bytes {
		frame.extend(bytes);
	}
	Some(frame)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn packet_keeps_control_utf8_and_binary_separate() {
		let cases = [None, Some(Vec::new()), Some(vec![0, 255, 42])];
		for bytes in cases {
			let reply = WireReply {
				json: String::from("{\"name\":\"星\"}"),
				bytes: bytes.clone(),
			};
			let frame = encode(reply).expect("small reply encodes");
			let decoded = decode(&frame).expect("same format");
			assert_eq!(decoded.0, "{\"name\":\"星\"}");
			assert_eq!(decoded.1, bytes.as_deref());
		}
	}

	#[test]
	fn packet_rejects_bad_length_flags_text_and_trailing_bytes() {
		for invalid in [
			&[0, 3, 0, 0, 0, b'{'][..],
			&[0, 0, 0, 0, 0, b'{'],
			&[0, 1, 0, 0, 0, 255],
			&[2, 0, 0, 0, 0],
			&[0, 0, 0],
			&[0, 255, 255, 255, 255],
		] {
			assert!(decode(invalid).is_none(), "{invalid:?}");
		}
	}
}
