#![forbid(unsafe_code)]

use std::{error::Error, fmt};

/// Maximum number of paths carried by one instance request.
pub const MAX_PATHS: usize = 32;

/// Maximum size in bytes of a request header and body together.
pub const MAX_FRAME_LEN: usize = 64 * 1024;

pub(crate) const HEADER_LEN: usize = 10;
pub(crate) const ACK_LEN: usize = 7;

const VERSION: u8 = 1;
const REQUEST_MAGIC: [u8; 3] = *b"SBQ";
const ACK_MAGIC: [u8; 3] = *b"SBA";
const UNIT_BYTES: usize = size_of::<u16>();
const PATH_LENGTH_BYTES: usize = size_of::<u16>();
const MIN_PATH_BYTES: usize = PATH_LENGTH_BYTES + UNIT_BYTES;
const MAX_PAYLOAD_LEN: usize = MAX_FRAME_LEN - HEADER_LEN;

/// An invalid instance message or an input exceeding the protocol limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameError {
	/// The message does not have the expected request or admission magic.
	WrongMessage,
	/// The message version is unsupported.
	UnsupportedVersion(u8),
	/// The request exceeds the path count limit.
	TooManyPaths,
	/// The encoded request exceeds the byte limit.
	FrameTooLarge,
	/// The request header declares an impossible body length.
	InvalidBodyLength,
	/// The supplied bytes do not match the body length in the header.
	BodyLengthMismatch,
	/// The body ends before the indicated path is complete.
	TruncatedPath { index: usize },
	/// A path contains no UTF-16 units.
	EmptyPath { index: usize },
	/// A path contains a NUL code unit.
	NulInPath { index: usize },
	/// The body contains bytes after all declared paths.
	TrailingBody,
	/// The reported queue counts exceed the request limit.
	TooManyAdmissions,
	/// The reported queue counts do not account for every submitted path.
	AdmissionCountMismatch { expected: usize, reported: usize },
}

impl fmt::Display for FrameError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::WrongMessage => formatter.write_str("unexpected instance message"),
			Self::UnsupportedVersion(version) => {
				write!(formatter, "unsupported instance message version {version}")
			}
			Self::TooManyPaths => formatter.write_str("instance request contains too many paths"),
			Self::FrameTooLarge => formatter.write_str("instance request exceeds the byte limit"),
			Self::InvalidBodyLength => formatter.write_str("invalid instance request body length"),
			Self::BodyLengthMismatch => {
				formatter.write_str("instance request body length does not match its header")
			}
			Self::TruncatedPath { index } => {
				write!(formatter, "instance path {index} is truncated")
			}
			Self::EmptyPath { index } => write!(formatter, "instance path {index} is empty"),
			Self::NulInPath { index } => write!(formatter, "instance path {index} contains NUL"),
			Self::TrailingBody => formatter.write_str("instance request contains trailing bytes"),
			Self::TooManyAdmissions => {
				formatter.write_str("instance admission exceeds the path limit")
			}
			Self::AdmissionCountMismatch { expected, reported } => write!(
				formatter,
				"instance admission reports {reported} paths for a request containing {expected}"
			),
		}
	}
}

impl Error for FrameError {}

/// One request to deliver paths to the running editor, or an empty request to focus it.
///
/// Paths retain their Windows UTF-16 code units, including unpaired surrogates.
/// Local path and extension policy belongs to the receiving application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenRequest {
	paths: Vec<Vec<u16>>,
	payload_len: usize,
}

impl OpenRequest {
	/// Checks path count, encoded size, and the contents of each path.
	///
	/// # Errors
	///
	/// Returns a typed error for an oversized request, an empty path, or a NUL code unit.
	pub fn new(paths: Vec<Vec<u16>>) -> Result<Self, FrameError> {
		if paths.len() > MAX_PATHS {
			return Err(FrameError::TooManyPaths);
		}
		let mut payload_len = 0usize;
		for (index, path) in paths.iter().enumerate() {
			payload_len = path
				.len()
				.checked_mul(UNIT_BYTES)
				.and_then(|bytes| bytes.checked_add(PATH_LENGTH_BYTES))
				.and_then(|bytes| bytes.checked_add(payload_len))
				.filter(|length| *length <= MAX_PAYLOAD_LEN)
				.ok_or(FrameError::FrameTooLarge)?;
			check_path(index, path.iter().copied())?;
		}
		Ok(Self { paths, payload_len })
	}

	/// Returns the paths in their original order and spelling.
	#[must_use]
	pub fn paths(&self) -> &[Vec<u16>] {
		&self.paths
	}

	pub(crate) fn encode(&self) -> Vec<u8> {
		let mut frame = Vec::with_capacity(HEADER_LEN + self.payload_len);
		frame.extend_from_slice(&REQUEST_MAGIC);
		frame.push(VERSION);
		// Construction bounds the count to MAX_PATHS and the body to MAX_PAYLOAD_LEN.
		frame.extend_from_slice(&(self.paths.len() as u16).to_le_bytes());
		frame.extend_from_slice(&(self.payload_len as u32).to_le_bytes());
		for path in &self.paths {
			// Each unit uses two bytes, so a path in this frame fits in a u16 count.
			frame.extend_from_slice(&(path.len() as u16).to_le_bytes());
			for unit in path {
				frame.extend_from_slice(&unit.to_le_bytes());
			}
		}
		frame
	}
}

/// A checked request prefix from which the receiver may size its bounded read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RequestHeader {
	path_count: usize,
	payload_len: usize,
}

impl RequestHeader {
	pub(crate) fn decode(bytes: &[u8; HEADER_LEN]) -> Result<Self, FrameError> {
		let [
			first,
			second,
			third,
			version,
			count_low,
			count_high,
			length @ ..,
		] = *bytes;
		check_prefix([first, second, third], version, REQUEST_MAGIC)?;
		let path_count = usize::from(u16::from_le_bytes([count_low, count_high]));
		if path_count > MAX_PATHS {
			return Err(FrameError::TooManyPaths);
		}
		let payload_len =
			usize::try_from(u32::from_le_bytes(length)).map_err(|_| FrameError::FrameTooLarge)?;
		if payload_len > MAX_PAYLOAD_LEN {
			return Err(FrameError::FrameTooLarge);
		}
		if !payload_len.is_multiple_of(UNIT_BYTES)
			|| (path_count == 0) != (payload_len == 0)
			|| payload_len < path_count * MIN_PATH_BYTES
		{
			return Err(FrameError::InvalidBodyLength);
		}
		Ok(Self {
			path_count,
			payload_len,
		})
	}

	pub(crate) fn payload_len(self) -> usize {
		self.payload_len
	}

	pub(crate) fn decode_body(self, payload: &[u8]) -> Result<OpenRequest, FrameError> {
		if self.payload_len != payload.len() {
			return Err(FrameError::BodyLengthMismatch);
		}

		// Verify all byte ranges and code units before allocating decoded paths.
		let mut path_bodies: [&[u8]; MAX_PATHS] = [&[]; MAX_PATHS];
		let mut remaining = payload;
		for (index, body) in path_bodies[..self.path_count].iter_mut().enumerate() {
			let (length, after_length) = remaining
				.split_first_chunk::<PATH_LENGTH_BYTES>()
				.ok_or(FrameError::TruncatedPath { index })?;
			let path_bytes = usize::from(u16::from_le_bytes(*length)) * UNIT_BYTES;
			let (raw, next) = after_length
				.split_at_checked(path_bytes)
				.ok_or(FrameError::TruncatedPath { index })?;
			check_path(index, units(raw))?;
			*body = raw;
			remaining = next;
		}
		if !remaining.is_empty() {
			return Err(FrameError::TrailingBody);
		}

		let paths = path_bodies[..self.path_count]
			.iter()
			.map(|raw| units(raw).collect())
			.collect();
		Ok(OpenRequest {
			paths,
			payload_len: self.payload_len,
		})
	}
}

fn check_path(
	index: usize,
	mut path: impl ExactSizeIterator<Item = u16>,
) -> Result<(), FrameError> {
	if path.len() == 0 {
		return Err(FrameError::EmptyPath { index });
	}
	if path.any(|unit| unit == 0) {
		return Err(FrameError::NulInPath { index });
	}
	Ok(())
}

fn units(bytes: &[u8]) -> impl ExactSizeIterator<Item = u16> + '_ {
	bytes
		.as_chunks::<UNIT_BYTES>()
		.0
		.iter()
		.copied()
		.map(u16::from_le_bytes)
}

fn check_prefix(magic: [u8; 3], version: u8, expected_magic: [u8; 3]) -> Result<(), FrameError> {
	if magic != expected_magic {
		return Err(FrameError::WrongMessage);
	}
	if version != VERSION {
		return Err(FrameError::UnsupportedVersion(version));
	}
	Ok(())
}

/// The receiver's accounting of path admission to its pending-open queue.
///
/// These counts report queue admission only. They do not confirm file access,
/// bank validation, or an editor change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueAdmission {
	queued: u8,
	already_pending: u8,
	refused: u8,
}

impl QueueAdmission {
	/// Checks that the combined counts fit one request.
	///
	/// # Errors
	///
	/// Returns an error when the counts exceed the maximum request path count.
	pub fn new(queued: u8, already_pending: u8, refused: u8) -> Result<Self, FrameError> {
		let admission = Self {
			queued,
			already_pending,
			refused,
		};
		if admission.total() > MAX_PATHS {
			return Err(FrameError::TooManyAdmissions);
		}
		Ok(admission)
	}

	/// Returns the number of new pending-open entries.
	#[must_use]
	pub fn queued(self) -> u8 {
		self.queued
	}

	/// Returns the number of paths that were already pending.
	#[must_use]
	pub fn already_pending(self) -> u8 {
		self.already_pending
	}

	/// Returns the number of paths the receiver did not queue.
	#[must_use]
	pub fn refused(self) -> u8 {
		self.refused
	}

	/// Returns the number of paths accounted for by this admission.
	#[must_use]
	pub fn total(self) -> usize {
		usize::from(self.queued) + usize::from(self.already_pending) + usize::from(self.refused)
	}

	pub(crate) fn encode(self) -> [u8; ACK_LEN] {
		let [first, second, third] = ACK_MAGIC;
		[
			first,
			second,
			third,
			VERSION,
			self.queued,
			self.already_pending,
			self.refused,
		]
	}

	pub(crate) fn decode(bytes: &[u8; ACK_LEN], request_count: usize) -> Result<Self, FrameError> {
		let [
			first,
			second,
			third,
			version,
			queued,
			already_pending,
			refused,
		] = *bytes;
		check_prefix([first, second, third], version, ACK_MAGIC)?;
		let admission = Self::new(queued, already_pending, refused)?;
		let reported = admission.total();
		if reported != request_count {
			return Err(FrameError::AdmissionCountMismatch {
				expected: request_count,
				reported,
			});
		}
		Ok(admission)
	}
}

#[cfg(test)]
mod tests;
