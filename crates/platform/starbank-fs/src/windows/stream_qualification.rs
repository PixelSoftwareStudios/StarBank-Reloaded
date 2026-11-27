//! Opt-in native stream query on a disposable local NTFS fixture.

use std::{
	error::Error,
	ffi::OsStr,
	fs::{self, File, OpenOptions},
	io::{self, Read, Write},
	mem::{align_of, offset_of},
	os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _},
	path::{Path, PathBuf},
	ptr,
};

use windows_sys::{
	Wdk::Storage::FileSystem::{
		FILE_STREAM_INFORMATION, FileStreamInformation, NtQueryInformationFile,
	},
	Win32::{
		Foundation::{ERROR_SHARING_VIOLATION, STATUS_PENDING, STATUS_SUCCESS},
		System::IO::IO_STATUS_BLOCK,
	},
};

use super::{
	DirectoryScope, NO_REPARSE_OR_RECALL, Observation, read_snapshot,
	rename_qualification::{prepare_reduced_fixture, with_reduced_token},
};

mod records;

const BANK_NAME: &str = "StreamFixture.SC2Bank";
const DEFAULT_STREAM: &str = "::$DATA";
const WRITTEN_STREAM: &str = ":Synthetic É:$DATA";
const EMPTY_STREAM: &str = ":$DATA:$DATA";
const BANK: &[u8] = b"<Bank><!-- unnamed data fixture --></Bank>";
const EXTRA: &[u8] = b"independent named stream fixture";

#[repr(C, align(8))]
struct Buffer([u8; records::MAX_BUFFER_BYTES]);

const _: () = {
	assert!(align_of::<Buffer>() == records::RECORD_ALIGNMENT);
	assert!(align_of::<Buffer>() >= align_of::<FILE_STREAM_INFORMATION>());
	assert!(offset_of!(FILE_STREAM_INFORMATION, NextEntryOffset) == records::NEXT_OFFSET);
	assert!(offset_of!(FILE_STREAM_INFORMATION, StreamNameLength) == records::NAME_LENGTH_OFFSET);
	assert!(offset_of!(FILE_STREAM_INFORMATION, StreamSize) == records::STREAM_SIZE_OFFSET);
	assert!(offset_of!(FILE_STREAM_INFORMATION, StreamName) == records::NAME_OFFSET);
};

#[test]
#[ignore = "requires native Windows with an ordinary local NTFS temporary directory"]
fn local_ntfs_held_file_stream_inventory() -> Result<(), Box<dyn Error>> {
	let temporary = tempfile::TempDir::new()?;
	let root = prepare_reduced_fixture(&temporary)?;
	with_reduced_token(move || qualify(&root))?;
	temporary.close()?;
	Ok(())
}

fn qualify(root: &Path) -> Result<(), Box<dyn Error>> {
	let path = root.join(BANK_NAME);
	write_new(&path, BANK)?;
	let original = read_snapshot(&path, BANK.len())?;
	assert_eq!(original.bytes(), BANK);

	let scope = DirectoryScope::open(&path)?;
	let mut held = scope.open_file(OsStr::new(BANK_NAME))?;
	let before = Observation::read(&held)?;
	assert_eq!(
		before.identity,
		original.fingerprint().windows.file.identity
	);
	assert_streams(&held, &[(DEFAULT_STREAM, BANK.len())])?;
	let default_writer = OpenOptions::new()
		.write(true)
		.custom_flags(NO_REPARSE_OR_RECALL)
		.open(&path);
	assert_eq!(
		default_writer.err().and_then(|error| error.raw_os_error()),
		Some(ERROR_SHARING_VIOLATION.cast_signed()),
		"the held unnamed stream must refuse a second unnamed-stream writer"
	);

	// FILE_SHARE_READ denies writes to the unnamed stream. Named data streams have their
	// own sharing state. Exercise the normal held handle and a distinct named-stream open.
	let named = stream_path(&path, WRITTEN_STREAM);
	write_new(&named, EXTRA).inspect_err(|error| {
		eprintln!("Named stream creation with held default reader: {error:?}");
	})?;
	assert_streams(
		&held,
		&[(DEFAULT_STREAM, BANK.len()), (WRITTEN_STREAM, EXTRA.len())],
	)?;
	let empty = stream_path(&path, EMPTY_STREAM);
	write_new(&empty, b"")?;
	let expected = [
		(DEFAULT_STREAM, BANK.len()),
		(WRITTEN_STREAM, EXTRA.len()),
		(EMPTY_STREAM, 0),
	];
	assert_streams(&held, &expected)?;

	let after = Observation::read(&held)?;
	assert_eq!(after.identity, before.identity);
	let mut held_bytes = Vec::new();
	(&mut held)
		.take(BANK.len() as u64 + 1)
		.read_to_end(&mut held_bytes)?;
	assert_eq!(held_bytes, BANK);
	assert_eq!(fs::read(&named)?, EXTRA);
	assert!(fs::read(&empty)?.is_empty());

	let current = read_snapshot(&path, BANK.len())?;
	assert_eq!(current.fingerprint().windows.file.identity, before.identity);
	assert_eq!(current.bytes(), BANK);
	eprintln!(
		"NTFS held file: named stream creation accepted; stream_counts=1/2/3; modified_changed={}, change_stamp_changed={}, snapshot_fingerprint_changed={}",
		after.modified != before.modified,
		after.changed != before.changed,
		current.fingerprint() != original.fingerprint()
	);
	Ok(())
}

fn stream_path(file: &Path, suffix: &str) -> PathBuf {
	let mut stream = file.as_os_str().to_os_string();
	stream.push(suffix);
	PathBuf::from(stream)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
	let mut file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.custom_flags(NO_REPARSE_OR_RECALL)
		.open(path)?;
	file.write_all(bytes)?;
	file.sync_all()
}

fn assert_streams(file: &File, expected: &[(&str, usize)]) -> io::Result<()> {
	let found = query_streams(file)?;
	assert_eq!(found.len(), expected.len(), "complete stream inventory");
	for &(name, size) in expected {
		assert_eq!(
			found
				.iter()
				.filter(|stream| stream.name.iter().copied().eq(name.encode_utf16())
					&& stream.size == size as u64)
				.count(),
			1,
			"expected exactly one matching fixture stream"
		);
	}
	Ok(())
}

fn query_streams(file: &File) -> io::Result<Vec<records::Stream>> {
	let mut buffer = Buffer([0; records::MAX_BUFFER_BYTES]);
	let mut completion = IO_STATUS_BLOCK::default();
	completion.Anonymous.Status = STATUS_PENDING;
	// SAFETY: all callers pass the live controlled fixture file opened by
	// DirectoryScope::open_file. The cap-primitives Windows open uses SYNCHRONIZE and
	// FILE_SYNCHRONOUS_IO_NONALERT. The status block and aligned initialized byte
	// buffer are writable for the full call. No variable-length structure is cast.
	let status = unsafe {
		NtQueryInformationFile(
			file.as_raw_handle(),
			&raw mut completion,
			ptr::from_mut(&mut buffer).cast(),
			records::MAX_BUFFER_BYTES as u32,
			FileStreamInformation,
		)
	};
	if status == STATUS_PENDING {
		// Do not drop live kernel output storage if the synchronous invariant breaks.
		std::process::abort();
	}
	if status != STATUS_SUCCESS {
		return Err(io::Error::other(format!(
			"query held NTFS streams: NTSTATUS {status:#010x}"
		)));
	}
	// SAFETY: the query writes the status member of its initialized SDK status block.
	let completed = unsafe { completion.Anonymous.Status };
	if completed != STATUS_SUCCESS {
		return Err(io::Error::other(format!(
			"query held NTFS streams: completion status {completed:#010x}"
		)));
	}
	let written = buffer.0.get(..completion.Information).ok_or_else(|| {
		io::Error::new(
			io::ErrorKind::InvalidData,
			"NTFS stream result exceeds its buffer",
		)
	})?;
	records::parse(written)
}
