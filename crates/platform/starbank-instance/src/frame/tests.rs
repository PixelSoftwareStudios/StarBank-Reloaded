use super::*;

fn header(path_count: u16, payload_len: u32) -> [u8; HEADER_LEN] {
	let [count_low, count_high] = path_count.to_le_bytes();
	let [length_0, length_1, length_2, length_3] = payload_len.to_le_bytes();
	[
		b'S', b'B', b'Q', 1, count_low, count_high, length_0, length_1, length_2, length_3,
	]
}

fn decode(frame: &[u8]) -> Result<OpenRequest, FrameError> {
	let (prefix, payload) = frame
		.split_first_chunk::<HEADER_LEN>()
		.expect("complete fixed-size test header");
	let request = RequestHeader::decode(prefix)?;
	assert_eq!(request.payload_len(), payload.len());
	request.decode_body(payload)
}

#[test]
fn request_version_and_little_endian_windows_code_units_have_an_exact_wire_form() {
	let spelling = vec![0x0041, 0xd800, 0x0042, 0xdc00];
	let request = OpenRequest::new(vec![spelling.clone()]).unwrap();
	let expected = [
		b'S', b'B', b'Q', 1, 1, 0, 10, 0, 0, 0, 4, 0, 0x41, 0, 0, 0xd8, 0x42, 0, 0, 0xdc,
	];
	assert_eq!(request.encode(), expected);
	assert_eq!(decode(&expected).unwrap().paths(), &[spelling]);
}

#[test]
fn empty_request_uses_the_same_header_and_carries_no_body() {
	let request = OpenRequest::new(Vec::new()).unwrap();
	assert_eq!(request.encode(), header(0, 0));
	assert!(decode(&request.encode()).unwrap().paths().is_empty());
}

#[test]
fn request_at_byte_and_path_limits_preserves_every_unit_and_order() {
	let units_at_limit = (MAX_FRAME_LEN - HEADER_LEN - PATH_LENGTH_BYTES) / UNIT_BYTES;
	let path = vec![0xffff; units_at_limit];
	let request = OpenRequest::new(vec![path]).unwrap();
	let frame = request.encode();
	assert_eq!(frame.len(), MAX_FRAME_LEN);
	assert_eq!(decode(&frame).unwrap(), request);
	assert_eq!(
		OpenRequest::new(vec![vec![0x0041; units_at_limit + 1]]),
		Err(FrameError::FrameTooLarge)
	);
	assert_eq!(
		OpenRequest::new(vec![vec![0x0041; units_at_limit - 1], vec![0x0042]]),
		Err(FrameError::FrameTooLarge)
	);

	let paths = (1..=MAX_PATHS)
		.rev()
		.map(|value| vec![u16::try_from(value).unwrap()])
		.collect::<Vec<_>>();
	let request = OpenRequest::new(paths.clone()).unwrap();
	assert_eq!(decode(&request.encode()).unwrap().paths(), paths);
	assert_eq!(
		OpenRequest::new(vec![vec![0x0041]; MAX_PATHS + 1]),
		Err(FrameError::TooManyPaths)
	);
}

#[test]
fn header_rejects_unsupported_or_impossible_requests_before_the_body_is_read() {
	let mut wrong_magic = header(0, 0);
	wrong_magic[2] = b'A';
	let mut wrong_version = header(0, 0);
	wrong_version[3] = 2;
	for (bytes, error) in [
		(wrong_magic, FrameError::WrongMessage),
		(wrong_version, FrameError::UnsupportedVersion(2)),
		(header(u16::MAX, 0), FrameError::TooManyPaths),
		(header(0, u32::MAX), FrameError::FrameTooLarge),
		(header(0, 2), FrameError::InvalidBodyLength),
		(header(1, 0), FrameError::InvalidBodyLength),
		(header(1, 5), FrameError::InvalidBodyLength),
		(header(2, 6), FrameError::InvalidBodyLength),
	] {
		assert_eq!(RequestHeader::decode(&bytes), Err(error));
	}
}

#[test]
fn decode_requires_exact_body_and_complete_nonempty_nul_free_paths() {
	let one_path = RequestHeader::decode(&header(1, 4)).unwrap();
	assert_eq!(
		one_path.decode_body(&[1, 0, 0x41]),
		Err(FrameError::BodyLengthMismatch)
	);
	assert_eq!(
		one_path.decode_body(&[1, 0, 0x41, 0, 0x42, 0]),
		Err(FrameError::BodyLengthMismatch)
	);
	assert_eq!(
		one_path.decode_body(&[0, 0, 0x41, 0]),
		Err(FrameError::EmptyPath { index: 0 })
	);
	assert_eq!(
		one_path.decode_body(&[1, 0, 0, 0]),
		Err(FrameError::NulInPath { index: 0 })
	);
	let two_paths = RequestHeader::decode(&header(2, 8)).unwrap();
	assert_eq!(
		two_paths.decode_body(&[1, 0, 0x41, 0, 2, 0, 0x42, 0]),
		Err(FrameError::TruncatedPath { index: 1 })
	);
	let trailing = RequestHeader::decode(&header(1, 6)).unwrap();
	assert_eq!(
		trailing.decode_body(&[1, 0, 0x41, 0, 1, 0]),
		Err(FrameError::TrailingBody)
	);
	assert_eq!(
		OpenRequest::new(vec![vec![0x0041], vec![]]),
		Err(FrameError::EmptyPath { index: 1 })
	);
	assert_eq!(
		OpenRequest::new(vec![vec![0x0041], vec![0, 0x0042]]),
		Err(FrameError::NulInPath { index: 1 })
	);
}

#[test]
fn admission_wire_form_accounts_for_all_requested_paths() {
	let expected = [b'S', b'B', b'A', 1, 2, 1, 3];
	let admission = QueueAdmission::new(2, 1, 3).unwrap();
	assert_eq!(admission.encode(), expected);
	let decoded = QueueAdmission::decode(&expected, 6).unwrap();
	assert_eq!(
		(
			decoded.queued(),
			decoded.already_pending(),
			decoded.refused()
		),
		(2, 1, 3)
	);
	let focus = QueueAdmission::new(0, 0, 0).unwrap();
	assert_eq!(QueueAdmission::decode(&focus.encode(), 0), Ok(focus));
	let full = QueueAdmission::new(30, 1, 1).unwrap();
	assert_eq!(QueueAdmission::decode(&full.encode(), MAX_PATHS), Ok(full));
	assert_eq!(
		QueueAdmission::decode(&expected, 5),
		Err(FrameError::AdmissionCountMismatch {
			expected: 5,
			reported: 6,
		})
	);
	assert_eq!(
		QueueAdmission::decode(&expected, 7),
		Err(FrameError::AdmissionCountMismatch {
			expected: 7,
			reported: 6,
		})
	);
}

#[test]
fn admission_rejects_wrong_kind_version_and_oversubscribed_counts() {
	assert_eq!(
		QueueAdmission::decode(&[b'S', b'B', b'Q', 1, 0, 0, 0], 0),
		Err(FrameError::WrongMessage)
	);
	assert_eq!(
		QueueAdmission::decode(&[b'S', b'B', b'A', 2, 0, 0, 0], 0),
		Err(FrameError::UnsupportedVersion(2))
	);
	assert_eq!(
		QueueAdmission::new(u8::MAX, u8::MAX, u8::MAX),
		Err(FrameError::TooManyAdmissions)
	);
	assert_eq!(
		QueueAdmission::decode(&[b'S', b'B', b'A', 1, 31, 1, 1], 32),
		Err(FrameError::TooManyAdmissions)
	);
}

#[test]
fn all_nonzero_utf16_code_units_round_trip_across_bounded_requests() {
	const UNITS_PER_PATH: usize = 257;

	// Reversing bits permutes the nonzero values and puts surrogate units among other values.
	let units: Vec<_> = (1..=u16::MAX).map(u16::reverse_bits).collect();
	for batch in units.chunks(MAX_PATHS * UNITS_PER_PATH) {
		let paths = batch.chunks(UNITS_PER_PATH).map(<[u16]>::to_vec).collect();
		let request = OpenRequest::new(paths).unwrap();
		let frame = request.encode();
		assert!(frame.len() <= MAX_FRAME_LEN);
		assert_eq!(decode(&frame).unwrap(), request);
	}
}

#[test]
fn every_truncated_prefix_is_rejected_even_with_an_adjusted_body_length() {
	let max_units = (MAX_FRAME_LEN - HEADER_LEN - PATH_LENGTH_BYTES) / UNIT_BYTES;
	let multiple = (1..=MAX_PATHS).map(|units| vec![0x0041; units]).collect();
	for paths in [multiple, vec![vec![0x0042; max_units]]] {
		let request = OpenRequest::new(paths).unwrap();
		let path_count = u16::try_from(request.paths().len()).unwrap();
		let frame = request.encode();
		let (prefix, payload) = frame.split_first_chunk::<HEADER_LEN>().unwrap();
		let declared = RequestHeader::decode(prefix).unwrap();

		for len in 0..payload.len() {
			let truncated = &payload[..len];
			assert_eq!(
				declared.decode_body(truncated),
				Err(FrameError::BodyLengthMismatch)
			);
			let shorter_header = header(path_count, u32::try_from(len).unwrap());
			match RequestHeader::decode(&shorter_header) {
				Ok(adjusted) => assert!(matches!(
					adjusted.decode_body(truncated),
					Err(FrameError::TruncatedPath { .. })
				)),
				Err(error) => assert_eq!(error, FrameError::InvalidBodyLength),
			}
		}
	}
}
