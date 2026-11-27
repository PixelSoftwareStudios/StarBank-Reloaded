//! Creation-time owner-only Windows staging security.

use std::{
	ffi::{OsStr, c_void},
	fs::File,
	io,
	mem::{offset_of, size_of},
	os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
	ptr::{self, NonNull},
	slice,
};

use windows_sys::Win32::{
	Foundation::{ERROR_NO_TOKEN, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, LocalFree},
	Security::{
		ACCESS_ALLOWED_ACE, ACE_HEADER,
		Authorization::{
			ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
			GetSecurityInfo, SE_FILE_OBJECT,
		},
		DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetAce,
		GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorGroup,
		GetSecurityDescriptorOwner, GetTokenInformation, IsValidAcl, OWNER_SECURITY_INFORMATION,
		PSECURITY_DESCRIPTOR, PSID, SE_DACL_AUTO_INHERIT_REQ, SE_DACL_AUTO_INHERITED,
		SE_DACL_DEFAULTED, SE_DACL_PRESENT, SE_DACL_PROTECTED, SE_GROUP_DEFAULTED,
		SE_OWNER_DEFAULTED, SE_SELF_RELATIVE, TOKEN_PRIMARY_GROUP, TOKEN_QUERY, TOKEN_USER,
		TokenPrimaryGroup, TokenUser,
	},
	Storage::FileSystem::{DELETE, FILE_ALL_ACCESS, READ_CONTROL, SYNCHRONIZE},
	System::Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
};

use crate::StorageError;

use super::DirectoryScope;

#[cfg(test)]
pub(super) mod diagnostics;
mod relative;

#[cfg(test)]
pub(super) use relative::{RelativeKind, create_relative};

const SECURITY_STRING_REVISION: u32 = 1;
const MAX_SID_STRING_UNITS: usize = 184;
const MAX_TOKEN_BYTES: usize = 512;
const SID_HEADER_BYTES: usize = 8;
const MAX_SUB_AUTHORITIES: u8 = 15;
const ALLOW_ACE_TYPE: u8 = 0;
const MAX_OBSERVED_ACES: u16 = 32;
const OWNER_GROUP_DACL: u32 =
	OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
// System audit ACL bits are outside the queried security information.
const CONTROL_MASK: u16 = SE_OWNER_DEFAULTED
	| SE_GROUP_DEFAULTED
	| SE_DACL_PRESENT
	| SE_DACL_DEFAULTED
	| SE_DACL_AUTO_INHERIT_REQ
	| SE_DACL_AUTO_INHERITED
	| SE_DACL_PROTECTED
	| SE_SELF_RELATIVE;
const PRIVATE_CONTROL: u16 = SE_DACL_PRESENT | SE_DACL_PROTECTED | SE_SELF_RELATIVE;
const PRIVATE_STAGE_ACCESS: u32 =
	GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL | SYNCHRONIZE;

/// Build and use the plan under the same effective Windows token.
pub(super) struct PrivateStageSecurity {
	descriptor: SecurityDescriptor,
	expected: SecurityObservation,
}

impl PrivateStageSecurity {
	pub(super) fn current() -> Result<Self, StorageError> {
		let token = EffectiveToken::current()?;
		let user = token.user()?;
		let group = token.primary_group()?;
		let descriptor =
			SecurityDescriptor::parse_sddl(&format!("O:{user}G:{group}D:P(A;;FA;;;{user})"))?;
		let expected = SecurityObservation::from_descriptor(&descriptor)?;
		let [owner_ace] = expected.dacl.as_slice() else {
			return Err(StorageError::UnsafeMetadata("invalid private stage DACL"));
		};
		if expected.owner != user
			|| expected.group != group
			|| expected.control != PRIVATE_CONTROL
			|| owner_ace.flags != 0
			|| owner_ace.mask != FILE_ALL_ACCESS
			|| owner_ace.sid != user
		{
			return Err(StorageError::UnsafeMetadata(
				"invalid private stage descriptor",
			));
		}
		Ok(Self {
			descriptor,
			expected,
		})
	}

	/// Returns the owned, empty file. The caller installs its cleanup guard before verification.
	pub(super) fn create(
		&self,
		scope: &DirectoryScope,
		leaf: &OsStr,
	) -> Result<File, StorageError> {
		relative::create_private_file(&scope.parent, leaf, PRIVATE_STAGE_ACCESS, &self.descriptor)
	}

	/// Call before the first payload write and again through freshly bound verification handles.
	pub(super) fn verify(&self, file: &File) -> Result<(), StorageError> {
		if SecurityObservation::read(file)? != self.expected {
			return Err(StorageError::UnsafeMetadata(
				"private stage security differs",
			));
		}
		Ok(())
	}
}

struct LocalAllocation(NonNull<c_void>);

impl LocalAllocation {
	fn new(pointer: *mut c_void) -> Result<Self, StorageError> {
		NonNull::new(pointer)
			.map(Self)
			.ok_or(StorageError::UnsafeMetadata(
				"missing native security allocation",
			))
	}

	fn as_ptr(&self) -> *mut c_void {
		self.0.as_ptr()
	}
}

impl Drop for LocalAllocation {
	fn drop(&mut self) {
		// SAFETY: ownership comes from a documented LocalFree-owning API.
		unsafe { LocalFree(self.0.as_ptr()) };
	}
}

pub(super) struct SecurityDescriptor(LocalAllocation);

impl SecurityDescriptor {
	#[cfg(test)]
	pub(super) fn from_sddl(sddl: &str) -> Result<Self, StorageError> {
		Self::parse_sddl(sddl)
	}

	fn parse_sddl(sddl: &str) -> Result<Self, StorageError> {
		let sddl: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
		let mut descriptor = ptr::null_mut();
		// SAFETY: NUL-terminated SDDL and output remain live for the synchronous call.
		win32("build Windows security descriptor", unsafe {
			ConvertStringSecurityDescriptorToSecurityDescriptorW(
				sddl.as_ptr(),
				SECURITY_STRING_REVISION,
				&mut descriptor,
				ptr::null_mut(),
			)
		})?;
		Ok(Self(LocalAllocation::new(descriptor)?))
	}

	fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
		self.0.as_ptr()
	}
}

#[derive(PartialEq, Eq)]
pub(super) struct AllowedAce {
	pub(super) flags: u8,
	pub(super) mask: u32,
	pub(super) sid: String,
}

#[derive(PartialEq, Eq)]
pub(super) struct SecurityObservation {
	pub(super) owner: String,
	pub(super) group: String,
	pub(super) dacl: Vec<AllowedAce>,
	pub(super) control: u16,
}

impl std::fmt::Debug for SecurityObservation {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter
			.debug_struct("SecurityObservation")
			.field("ace_count", &self.dacl.len())
			.field("control", &self.control)
			.finish_non_exhaustive()
	}
}

impl SecurityObservation {
	fn from_descriptor(descriptor: &SecurityDescriptor) -> Result<Self, StorageError> {
		let descriptor = descriptor.as_ptr();
		let (mut owner, mut group, mut dacl) = (ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
		let (mut defaulted, mut present, mut control, mut revision) = (0, 0, 0, 0);
		// SAFETY: descriptor is live and all output slots are writable.
		unsafe {
			win32(
				"query Windows file owner",
				GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted),
			)?;
			win32(
				"query Windows file group",
				GetSecurityDescriptorGroup(descriptor, &mut group, &mut defaulted),
			)?;
			win32(
				"query Windows file DACL",
				GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted),
			)?;
			win32(
				"query Windows security flags",
				GetSecurityDescriptorControl(descriptor, &mut control, &mut revision),
			)?;
		}
		// SAFETY: a non-null DACL returned by the API is valid for the lifetime of the descriptor.
		if present == 0 || dacl.is_null() || unsafe { IsValidAcl(dacl) } == 0 {
			return Err(StorageError::UnsafeMetadata(
				"file DACL is missing or invalid",
			));
		}
		// SAFETY: IsValidAcl checked the live ACL before reading its header.
		let count = unsafe { (*dacl).AceCount };
		if count > MAX_OBSERVED_ACES {
			return Err(StorageError::UnsafeMetadata(
				"file DACL exceeds the ACE limit",
			));
		}
		let mut entries = Vec::with_capacity(usize::from(count));
		for index in 0..count {
			let mut entry = ptr::null_mut();
			// SAFETY: GetAce checks the index within the verified ACL and writes our slot.
			win32("read Windows file ACE", unsafe {
				GetAce(dacl, u32::from(index), &mut entry)
			})?;
			// SAFETY: successful GetAce returns a live entry in the validated ACL.
			let header = unsafe { entry.cast::<ACE_HEADER>().read_unaligned() };
			const SID_OFFSET: usize = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
			let ace_bytes = usize::from(header.AceSize);
			if header.AceType != ALLOW_ACE_TYPE || ace_bytes < SID_OFFSET + SID_HEADER_BYTES {
				return Err(StorageError::UnsafeMetadata("unsupported file access ACE"));
			}
			// SAFETY: the header checks establish a complete mask and bounded SID header.
			let (mask, sid) = unsafe {
				let entry = entry.cast::<ACCESS_ALLOWED_ACE>();
				(
					(&raw const (*entry).Mask).read_unaligned(),
					(&raw mut (*entry).SidStart).cast::<u8>(),
				)
			};
			// SAFETY: the ACE header guaranteed at least SID_HEADER_BYTES after SID_OFFSET.
			let count = unsafe { *sid.add(1) };
			if !sid_fits(count, ace_bytes - SID_OFFSET) {
				return Err(StorageError::UnsafeMetadata(
					"file ACE contains a truncated SID",
				));
			}
			entries.push(AllowedAce {
				flags: header.AceFlags,
				mask,
				// SAFETY: the bounded SID stays live in the validated ACE.
				sid: unsafe { sid_string(sid.cast()) }?,
			});
		}
		Ok(Self {
			// SAFETY: the borrowed descriptor owns the SIDs returned by the SDK.
			owner: unsafe { sid_string(owner) }?,
			group: unsafe { sid_string(group) }?,
			dacl: entries,
			control: control & CONTROL_MASK,
		})
	}

	pub(super) fn read(file: &File) -> Result<Self, StorageError> {
		let mut descriptor = ptr::null_mut();
		// SAFETY: the file handle stays open and the API returns an owned security descriptor.
		let code = unsafe {
			GetSecurityInfo(
				file.as_raw_handle(),
				SE_FILE_OBJECT,
				OWNER_GROUP_DACL,
				ptr::null_mut(),
				ptr::null_mut(),
				ptr::null_mut(),
				ptr::null_mut(),
				&mut descriptor,
			)
		};
		if code != ERROR_SUCCESS {
			return Err(StorageError::io(
				"read Windows file security",
				io::Error::from_raw_os_error(code.cast_signed()),
			));
		}
		let descriptor = SecurityDescriptor(LocalAllocation::new(descriptor)?);
		Self::from_descriptor(&descriptor)
	}
}

pub(super) struct EffectiveToken(OwnedHandle);

impl EffectiveToken {
	pub(super) fn current() -> Result<Self, StorageError> {
		let mut token = ptr::null_mut();
		// SAFETY: current-thread pseudo handle stays valid. OpenAsSelf only selects
		// the context for opening the effective token. It does not change that token.
		if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } == 0 {
			let error = io::Error::last_os_error();
			if error.raw_os_error() != Some(ERROR_NO_TOKEN.cast_signed()) {
				return Err(StorageError::io("open Windows thread token", error));
			}
			// SAFETY: no thread token exists. The current-process pseudo handle and output
			// slot are live, and only TOKEN_QUERY is requested.
			win32("open Windows process token", unsafe {
				OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
			})?;
		}
		if token.is_null() {
			return Err(StorageError::UnsafeMetadata(
				"missing effective Windows token",
			));
		}
		// SAFETY: successful token open transferred exactly one owned handle.
		Ok(Self(unsafe { OwnedHandle::from_raw_handle(token) }))
	}

	pub(super) fn user(&self) -> Result<String, StorageError> {
		self.sid(TokenSid::User)
	}

	pub(super) fn primary_group(&self) -> Result<String, StorageError> {
		self.sid(TokenSid::PrimaryGroup)
	}

	#[cfg(test)]
	pub(super) fn default_owner(&self) -> Result<String, StorageError> {
		self.sid(TokenSid::DefaultOwner)
	}

	#[cfg(test)]
	pub(super) fn selected_type(
		&self,
	) -> Result<windows_sys::Win32::Security::TOKEN_TYPE, StorageError> {
		use windows_sys::Win32::Security::{
			TOKEN_TYPE, TokenImpersonation, TokenPrimary, TokenType,
		};

		let mut kind: TOKEN_TYPE = 0;
		let mut returned = 0;
		let size = size_of::<TOKEN_TYPE>() as u32;
		// SAFETY: the token has TOKEN_QUERY and TokenType writes one TOKEN_TYPE.
		win32("read effective Windows token type", unsafe {
			GetTokenInformation(
				self.0.as_raw_handle(),
				TokenType,
				(&raw mut kind).cast(),
				size,
				&mut returned,
			)
		})?;
		if returned != size || ![TokenPrimary, TokenImpersonation].contains(&kind) {
			return Err(StorageError::UnsafeMetadata(
				"invalid effective Windows token type",
			));
		}
		Ok(kind)
	}

	fn sid(&self, kind: TokenSid) -> Result<String, StorageError> {
		let mut buffer = [0usize; MAX_TOKEN_BYTES / size_of::<usize>()];
		let mut returned = 0;
		let (class, minimum) = kind.layout();
		// SAFETY: the live effective token has TOKEN_QUERY. The usize array is aligned for
		// the SDK records and provides MAX_TOKEN_BYTES writable bytes.
		win32("read Windows token SID", unsafe {
			GetTokenInformation(
				self.0.as_raw_handle(),
				class,
				buffer.as_mut_ptr().cast(),
				MAX_TOKEN_BYTES as u32,
				&mut returned,
			)
		})?;
		let returned = returned as usize;
		if !(minimum..=MAX_TOKEN_BYTES).contains(&returned) {
			return Err(StorageError::UnsafeMetadata(
				"invalid Windows token SID size",
			));
		}
		let base = buffer.as_ptr().cast::<u8>();
		// SAFETY: layout() and the returned length establish the complete SDK pointer field.
		let sid = unsafe {
			match kind {
				TokenSid::User => (*base.cast::<TOKEN_USER>()).User.Sid,
				TokenSid::PrimaryGroup => (*base.cast::<TOKEN_PRIMARY_GROUP>()).PrimaryGroup,
				#[cfg(test)]
				TokenSid::DefaultOwner => (*base.cast::<windows_sys::Win32::Security::TOKEN_OWNER>()).Owner,
			}
		};
		let offset = sid
			.addr()
			.checked_sub(base.addr())
			.filter(|&offset| {
				offset >= minimum
					&& returned
						.checked_sub(offset)
						.is_some_and(|bytes| bytes >= SID_HEADER_BYTES)
			})
			.ok_or(StorageError::UnsafeMetadata(
				"Windows token SID is outside its buffer",
			))?;
		// SAFETY: offset is verified against the returned buffer before reconstructing the pointer.
		let count = unsafe { *base.add(offset + 1) };
		if !sid_fits(count, returned - offset) {
			return Err(StorageError::UnsafeMetadata(
				"Windows token SID is truncated",
			));
		}
		// SAFETY: the header and complete SID were checked within the live token allocation.
		unsafe { sid_string(base.add(offset).cast_mut().cast()) }
	}
}

enum TokenSid {
	User,
	PrimaryGroup,
	#[cfg(test)]
	DefaultOwner,
}

impl TokenSid {
	fn layout(&self) -> (i32, usize) {
		match self {
			Self::User => (TokenUser, size_of::<TOKEN_USER>()),
			Self::PrimaryGroup => (TokenPrimaryGroup, size_of::<TOKEN_PRIMARY_GROUP>()),
			#[cfg(test)]
			Self::DefaultOwner => (
				windows_sys::Win32::Security::TokenOwner,
				size_of::<windows_sys::Win32::Security::TOKEN_OWNER>(),
			),
		}
	}
}

fn sid_fits(count: u8, bytes: usize) -> bool {
	count <= MAX_SUB_AUTHORITIES
		&& SID_HEADER_BYTES + usize::from(count) * size_of::<u32>() <= bytes
}

/// # Safety
/// A non-null pointer must address a complete SID that stays live for the call.
unsafe fn sid_string(sid: PSID) -> Result<String, StorageError> {
	if sid.is_null() {
		return Err(StorageError::UnsafeMetadata(
			"missing Windows security identity",
		));
	}
	let mut output = ptr::null_mut();
	// SAFETY: the caller passed a SID in a live descriptor, checked ACE, or token buffer.
	win32("format Windows security identity", unsafe {
		ConvertSidToStringSidW(sid, &mut output)
	})?;
	let _allocation = LocalAllocation::new(output.cast())?;
	for length in 0..MAX_SID_STRING_UNITS {
		// SAFETY: the API returned an owned NUL-terminated string. Stop on that terminator.
		if unsafe { *output.add(length) } == 0 {
			// SAFETY: all length units precede the terminator in the live allocation.
			let units = unsafe { slice::from_raw_parts(output, length) };
			return String::from_utf16(units)
				.map_err(|_| StorageError::UnsafeMetadata("invalid Windows security identity"));
		}
	}
	Err(StorageError::UnsafeMetadata(
		"Windows security identity exceeds its limit",
	))
}

fn win32(operation: &'static str, result: i32) -> Result<(), StorageError> {
	if result == 0 {
		Err(StorageError::io(operation, io::Error::last_os_error()))
	} else {
		Ok(())
	}
}
