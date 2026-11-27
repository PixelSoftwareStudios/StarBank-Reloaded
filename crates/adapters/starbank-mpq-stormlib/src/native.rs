//! Narrow bindings for the v9.40 C ABI on arm64 Darwin.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::ptr::{self, NonNull};
use std::sync::Mutex;

use starbank_mpq::V1Addition;

use crate::scratch::Scratch;
use crate::{AppendError, NativeOperation};

type Handle = *mut c_void;

const MPQ_OPEN_NO_HEADER_SEARCH: u32 = 0x0004_0000;
const DEFAULT_PRIORITY: u32 = 0;
const ERROR_FILE_NOT_FOUND: u32 = libc::ENOENT as u32;
const STORED_NEW_MEMBER_FLAGS: u32 = 0;
const NEUTRAL_LOCALE_AND_PLATFORM: u32 = 0;
const NO_FILE_TIME: u64 = 0;
const NO_COMPRESSION: u32 = 0;

// v9.40 initializes process-wide hash tables without synchronization. Every C call made
// by this adapter shares this guard, including handle cleanup before it is released.
static NATIVE_STATE: Mutex<()> = Mutex::new(());

// The pinned header uses C linkage, native bool, and an empty WINAPI on Darwin.
unsafe extern "C" {
	fn SFileOpenArchive(path: *const c_char, priority: u32, flags: u32, out: *mut Handle) -> bool;
	fn SFileHasFile(archive: Handle, name: *const c_char) -> bool;
	fn SFileCreateFile(
		archive: Handle,
		name: *const c_char,
		file_time: u64,
		size: u32,
		locale: u32,
		flags: u32,
		out: *mut Handle,
	) -> bool;
	fn SFileWriteFile(member: Handle, bytes: *const c_void, size: u32, compression: u32) -> bool;
	fn SFileFinishFile(member: Handle) -> bool;
	fn SFileFlushArchive(archive: Handle) -> bool;
	fn SFileCloseArchive(archive: Handle) -> bool;
	fn SErrGetLastError() -> u32;
}

pub(super) fn write_members(
	scratch: &Scratch,
	additions: &[V1Addition<'_>],
) -> Result<(), AppendError> {
	let _exclusive = NATIVE_STATE
		.lock()
		.map_err(|_| AppendError::NativeStateUnavailable)?;
	let mut archive = Archive::open(scratch)?;
	let update = additions
		.iter()
		.try_for_each(|addition| archive.add(addition))
		.and_then(|()| archive.flush());
	let close = archive.close();
	update.and(close)
}

struct Archive(NonNull<c_void>);

impl Archive {
	fn open(scratch: &Scratch) -> Result<Self, AppendError> {
		let mut handle = ptr::null_mut();
		// SAFETY: path comes from the private stage, is NUL-terminated, and out lives for the call.
		let opened = unsafe {
			SFileOpenArchive(
				scratch.path().as_ptr(),
				DEFAULT_PRIORITY,
				MPQ_OPEN_NO_HEADER_SEARCH,
				&mut handle,
			)
		};
		check(opened, NativeOperation::Open, None)?;
		non_null(handle, NativeOperation::Open).map(Self)
	}

	fn add(&mut self, addition: &V1Addition<'_>) -> Result<(), AppendError> {
		let name = CString::new(addition.name)
			.map_err(|_| AppendError::InvalidMember(addition.name.to_owned()))?;
		let length = u32::try_from(addition.bytes.len())
			.map_err(|_| AppendError::InvalidMember(addition.name.to_owned()))?;
		// SAFETY: the archive handle is live and the member name is NUL-terminated.
		let exists = unsafe { SFileHasFile(self.0.as_ptr(), name.as_ptr()) };
		if exists {
			return Err(AppendError::ExistingMember(addition.name.to_owned()));
		}
		let code = last_error();
		if code != ERROR_FILE_NOT_FOUND {
			return Err(native_error(
				NativeOperation::CheckMember,
				Some(addition.name),
				code,
			));
		}

		let member = self.create_member(&name, addition.name, length)?;
		let write = member.write(addition);
		let finish = member.finish(addition.name);
		write.and(finish)
	}

	fn create_member(
		&mut self,
		name: &CStr,
		display_name: &str,
		length: u32,
	) -> Result<Member<'_>, AppendError> {
		let mut handle = ptr::null_mut();
		// SAFETY: the archive handle is live, name is NUL-terminated, and out lives for the call.
		// Zero requests a stored, non-replacing write. StormLib sets EXISTS internally,
		// and the same bit in the input flags would request REPLACEEXISTING.
		let created = unsafe {
			SFileCreateFile(
				self.0.as_ptr(),
				name.as_ptr(),
				NO_FILE_TIME,
				length,
				NEUTRAL_LOCALE_AND_PLATFORM,
				STORED_NEW_MEMBER_FLAGS,
				&mut handle,
			)
		};
		check(created, NativeOperation::CreateMember, Some(display_name))?;
		Ok(Member {
			handle: non_null(handle, NativeOperation::CreateMember)?,
			_archive: PhantomData,
		})
	}

	fn flush(&mut self) -> Result<(), AppendError> {
		// SAFETY: we own a live archive. Member's exclusive borrow prevents flushing while it is open.
		let flushed = unsafe { SFileFlushArchive(self.0.as_ptr()) };
		check(flushed, NativeOperation::Flush, None)
	}

	fn close(self) -> Result<(), AppendError> {
		let handle = self.0;
		std::mem::forget(self);
		// SAFETY: transfer our sole live archive handle to its consuming close operation.
		let closed = unsafe { SFileCloseArchive(handle.as_ptr()) };
		check(closed, NativeOperation::Close, None)
	}
}

impl Drop for Archive {
	fn drop(&mut self) {
		// SAFETY: this fallback runs only while we still own the live archive handle.
		// Called on unwinding; the enclosing private stage cannot be returned.
		unsafe { SFileCloseArchive(self.0.as_ptr()) };
	}
}

struct Member<'archive> {
	handle: NonNull<c_void>,
	_archive: PhantomData<&'archive mut Archive>,
}

impl Member<'_> {
	fn write(&self, addition: &V1Addition<'_>) -> Result<(), AppendError> {
		let Some((first, remaining)) = addition.bytes.split_first() else {
			return Ok(());
		};
		// v9.40 inspects only the first write buffer for incremental patch headers,
		// using an aligned C struct cast. One byte is below its detector's minimum;
		// subsequent bytes are copied into its own sector buffer without that cast.
		self.write_bytes(std::slice::from_ref(first), addition.name)?;
		if !remaining.is_empty() {
			self.write_bytes(remaining, addition.name)?;
		}
		Ok(())
	}

	fn write_bytes(&self, bytes: &[u8], name: &str) -> Result<(), AppendError> {
		let length =
			u32::try_from(bytes.len()).map_err(|_| AppendError::InvalidMember(name.to_owned()))?;
		// SAFETY: the member is live, the slice covers exactly length bytes, and the synchronous
		// call borrows them without taking ownership. Only stored writes are enabled.
		let written = unsafe {
			SFileWriteFile(
				self.handle.as_ptr(),
				bytes.as_ptr().cast(),
				length,
				NO_COMPRESSION,
			)
		};
		check(written, NativeOperation::WriteMember, Some(name))
	}

	fn finish(self, name: &str) -> Result<(), AppendError> {
		let handle = self.handle;
		std::mem::forget(self);
		// SAFETY: v9.40 consumes the live writable handle on success and on write failure.
		let finished = unsafe { SFileFinishFile(handle.as_ptr()) };
		check(finished, NativeOperation::FinishMember, Some(name))
	}
}

impl Drop for Member<'_> {
	fn drop(&mut self) {
		// SAFETY: ownership of the live writable handle has not transferred to finish.
		// The enclosing private stage cannot be returned if this runs during unwinding.
		unsafe { SFileFinishFile(self.handle.as_ptr()) };
	}
}

fn non_null(handle: Handle, operation: NativeOperation) -> Result<NonNull<c_void>, AppendError> {
	NonNull::new(handle).ok_or(AppendError::MissingHandle { operation })
}

fn last_error() -> u32 {
	// SAFETY: v9.40 retrieves thread-local status without accepting any pointers.
	unsafe { SErrGetLastError() }
}

fn check(
	success: bool,
	operation: NativeOperation,
	member: Option<&str>,
) -> Result<(), AppendError> {
	if success {
		Ok(())
	} else {
		Err(native_error(operation, member, last_error()))
	}
}

fn native_error(operation: NativeOperation, member: Option<&str>, code: u32) -> AppendError {
	AppendError::Native {
		operation,
		member: member.map(str::to_owned),
		code,
	}
}

#[cfg(test)]
mod tests {
	use std::ffi::CStr;

	use starbank_mpq::{V1_APPEND_ARCHIVE_LIMIT, V1Addition, V1AppendPlan};

	use super::{AppendError, Archive, NATIVE_STATE, NativeOperation, Scratch, write_members};

	const SOURCE: &[u8] = include_bytes!("../../starbank-mpq/tests/fixtures/v1-no-list.s2ma");

	#[test]
	fn native_guard_refuses_a_second_write_to_an_existing_member() {
		let addition = [V1Addition {
			name: "NativeCollision.txt",
			bytes: b"first payload",
		}];
		let plan = V1AppendPlan::prepare(SOURCE, &addition).expect("qualified synthetic source");
		let mut stage = Scratch::create(plan.source()).expect("private stage");
		write_members(&stage, plan.additions()).expect("first append");
		let original = stage
			.read_candidate(V1_APPEND_ARCHIVE_LIMIT)
			.expect("read first append");
		assert!(matches!(
			write_members(&stage, plan.additions()),
			Err(AppendError::ExistingMember(name)) if name == addition[0].name,
		));
		assert_eq!(
			stage
				.read_candidate(V1_APPEND_ARCHIVE_LIMIT)
				.expect("read after rejection"),
			original
		);
		stage.remove().expect("remove private stage");
	}

	#[test]
	fn native_create_rejects_existing_member_without_the_lookup_guard() {
		const NAME: &CStr = c"MapScript.galaxy";
		const EXISTING: V1Addition<'_> = V1Addition {
			name: "MapScript.galaxy",
			bytes: b"replacement payload",
		};
		let mut stage = Scratch::create(SOURCE).expect("private synthetic source");
		let attempt = {
			let _exclusive = NATIVE_STATE.lock().expect("exclusive native state");
			let mut archive = Archive::open(&stage).expect("open private archive");
			let length = u32::try_from(EXISTING.bytes.len()).expect("short synthetic payload");
			let result = archive
				.create_member(NAME, EXISTING.name, length)
				.and_then(|member| {
					let write = member.write(&EXISTING);
					write.and(member.finish(EXISTING.name))
				});
			archive.close().expect("close archive after create attempt");
			result
		};
		assert!(matches!(
			attempt,
			Err(AppendError::Native {
				operation: NativeOperation::CreateMember,
				member: Some(name),
				code,
			}) if name == EXISTING.name && code == libc::EEXIST as u32
		));
		assert_eq!(
			stage
				.read_candidate(V1_APPEND_ARCHIVE_LIMIT)
				.expect("read after rejected create"),
			SOURCE,
		);
		stage.remove().expect("remove private stage");
	}

	#[test]
	fn native_open_refuses_a_header_after_the_first_aligned_offset() {
		const HEADER_SEARCH_STEP: usize = 0x200;
		let mut wrapped = vec![0; HEADER_SEARCH_STEP];
		wrapped.extend_from_slice(SOURCE);
		let stage = Scratch::create(&wrapped).expect("private stage with displaced header");
		let attempt = {
			let _exclusive = NATIVE_STATE.lock().expect("exclusive native state");
			Archive::open(&stage).and_then(Archive::close)
		};
		assert!(matches!(
			attempt,
			Err(AppendError::Native {
				operation: NativeOperation::Open,
				code,
				..
			}) if code == libc::ENOTSUP as u32
		));
		stage.remove().expect("remove private stage");
	}

	#[test]
	fn native_finish_reports_an_incomplete_write_and_consumes_its_handle() {
		const NAME: &CStr = c"NativeShortWrite.txt";
		let plan_addition = [V1Addition {
			name: "NativeShortWrite.txt",
			bytes: b"a",
		}];
		let plan =
			V1AppendPlan::prepare(SOURCE, &plan_addition).expect("qualified synthetic source");
		let stage = Scratch::create(plan.source()).expect("private stage");
		let _exclusive = NATIVE_STATE.lock().expect("exclusive native state");
		let mut archive = Archive::open(&stage).expect("open private archive");
		let member = archive
			.create_member(NAME, plan_addition[0].name, 1)
			.expect("start one-byte member");
		assert!(matches!(
			member.finish(plan_addition[0].name),
			Err(AppendError::Native {
				operation: NativeOperation::FinishMember,
				..
			}),
		));
		archive
			.close()
			.expect("close archive after incomplete write");
		stage.remove().expect("remove private stage");
	}
}
