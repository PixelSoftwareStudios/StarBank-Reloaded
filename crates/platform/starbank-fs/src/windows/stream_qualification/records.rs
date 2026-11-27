//! Bounded parser for the populated portion of an NTFS stream information buffer.

#![forbid(unsafe_code)]

use std::io;

pub(super) const MAX_BUFFER_BYTES: usize = 64 * 1024;
pub(super) const RECORD_ALIGNMENT: usize = 8;
pub(super) const NEXT_OFFSET: usize = 0;
pub(super) const NAME_LENGTH_OFFSET: usize = 4;
pub(super) const STREAM_SIZE_OFFSET: usize = 8;
pub(super) const NAME_OFFSET: usize = 24;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Stream {
	pub(super) name: Vec<u16>,
	pub(super) size: u64,
}

pub(super) fn parse(mut written: &[u8]) -> io::Result<Vec<Stream>> {
	if written.len() > MAX_BUFFER_BYTES {
		return Err(invalid("NTFS stream information exceeds the probe bound"));
	}
	let mut streams = Vec::new();
	loop {
		let next = u32::from_le_bytes(field(written, NEXT_OFFSET)?) as usize;
		let name_length = u32::from_le_bytes(field(written, NAME_LENGTH_OFFSET)?) as usize;
		if name_length == 0 || !name_length.is_multiple_of(size_of::<u16>()) {
			return Err(invalid("invalid NTFS stream name byte length"));
		}
		let size = u64::try_from(i64::from_le_bytes(field(written, STREAM_SIZE_OFFSET)?))
			.map_err(|_| invalid("negative NTFS stream size"))?;
		let end = NAME_OFFSET
			.checked_add(name_length)
			.ok_or_else(|| invalid("NTFS stream name length overflow"))?;
		let name = written
			.get(NAME_OFFSET..end)
			.ok_or_else(|| invalid("truncated NTFS stream name"))?;
		let aligned_end = end.next_multiple_of(RECORD_ALIGNMENT);
		if next == 0 {
			if written.len() != end && written.len() != aligned_end {
				return Err(invalid(
					"unexpected bytes after the last NTFS stream record",
				));
			}
		} else if next != aligned_end || written.len().saturating_sub(next) < NAME_OFFSET {
			return Err(invalid("invalid NTFS stream record offset"));
		}

		let name: Vec<_> = name
			.as_chunks::<2>()
			.0
			.iter()
			.map(|&unit| u16::from_le_bytes(unit))
			.collect();
		if name.contains(&0) {
			return Err(invalid("NUL in counted NTFS stream name"));
		}
		streams.push(Stream { name, size });
		if next == 0 {
			return Ok(streams);
		}
		written = &written[next..];
	}
}

fn field<const N: usize>(written: &[u8], offset: usize) -> io::Result<[u8; N]> {
	written
		.get(offset..)
		.and_then(|suffix| suffix.first_chunk())
		.copied()
		.ok_or_else(|| invalid("truncated NTFS stream record"))
}

fn invalid(message: &'static str) -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests;
