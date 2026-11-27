//! Read-only reduced-token and directory security evidence for native qualification.

use super::{
	EffectiveToken, LocalAllocation, MAX_OBSERVED_ACES, MAX_TOKEN_BYTES, SID_HEADER_BYTES,
	SecurityObservation, win32,
};
use crate::StorageError;
use std::{
	fs::File,
	io,
	mem::{offset_of, size_of},
	os::windows::io::AsRawHandle as _,
	ptr, slice,
};
use windows_sys::Win32::{
	Foundation::ERROR_SUCCESS,
	Security::{
		ACE_HEADER,
		Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
		GetAce, GetSecurityDescriptorSacl, GetTokenInformation, INHERIT_ONLY_ACE, IsValidAcl,
		LABEL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SYSTEM_MANDATORY_LABEL_ACE,
		TOKEN_MANDATORY_LABEL, TOKEN_MANDATORY_POLICY, TokenIntegrityLevel, TokenMandatoryPolicy,
	},
	Storage::FileSystem::FILE_ALL_ACCESS,
};

const MANDATORY_LABEL_ACE_TYPE: u8 = 0x11;
// Revision 1, one subauthority, SECURITY_MANDATORY_LABEL_AUTHORITY (S-1-16).
const INTEGRITY_SID_HEADER: [u8; SID_HEADER_BYTES] = [1, 1, 0, 0, 0, 0, 0, 16];

pub(in crate::windows) fn report_token() {
	match EffectiveToken::current().and_then(|token| token_integrity(&token)) {
		Ok((rid, attributes, policy)) => {
			eprintln!(
				"NTFS access actor: integrity_rid={rid:#06x} attributes={attributes:#010x} mandatory_policy={policy:#010x}"
			);
		}
		Err(error) => eprintln!("NTFS access actor integrity unavailable: {error}"),
	}
}

pub(in crate::windows) fn report_descriptor(file: &File) {
	if let Err(error) = report_dacl(file) {
		eprintln!("NTFS access directory DACL unavailable: {error}");
	}
	match directory_label(file) {
		Ok(Some((rid, policy))) => {
			eprintln!(
				"NTFS access directory: integrity_rid={rid:#06x} mandatory_policy={policy:#010x}"
			);
		}
		Ok(None) => eprintln!("NTFS access directory: no explicit effective mandatory label"),
		Err(error) => eprintln!("NTFS access directory integrity unavailable: {error}"),
	}
}

fn report_dacl(file: &File) -> Result<(), StorageError> {
	let security = SecurityObservation::read(file)?;
	let token = EffectiveToken::current()?;
	let user = token.user()?;
	let group = token.primary_group()?;
	let protected = security.control & SE_DACL_PROTECTED != 0;
	let exact_user_full_access = protected
		&& matches!(security.dacl.as_slice(), [ace] if ace.sid == user && ace.mask == FILE_ALL_ACCESS
			&& u32::from(ace.flags) & INHERIT_ONLY_ACE == 0);
	eprintln!(
		"NTFS access directory: owner_is_user={} group_is_primary={} control={:#06x} protected={protected} exact_user_full_access={exact_user_full_access}",
		security.owner == user,
		security.group == group,
		security.control
	);
	for ace in security.dacl {
		eprintln!(
			"NTFS access directory ACE: trustee_is_user={} flags={:#04x} mask={:#010x}",
			ace.sid == user,
			ace.flags,
			ace.mask
		);
	}
	Ok(())
}

fn token_integrity(token: &EffectiveToken) -> Result<(u32, u32, u32), StorageError> {
	let mut words = [0usize; MAX_TOKEN_BYTES / size_of::<usize>()];
	let mut used = 0;
	// SAFETY: the effective held token has TOKEN_QUERY and the aligned buffer is writable.
	win32("query access-probe token integrity", unsafe {
		GetTokenInformation(
			token.0.as_raw_handle(),
			TokenIntegrityLevel,
			words.as_mut_ptr().cast(),
			MAX_TOKEN_BYTES as u32,
			&mut used,
		)
	})?;
	let used = used as usize;
	let minimum = size_of::<TOKEN_MANDATORY_LABEL>();
	if !(minimum..=MAX_TOKEN_BYTES).contains(&used) {
		return Err(StorageError::UnsafeMetadata(
			"invalid token integrity record length",
		));
	}
	let base = words.as_ptr().cast::<u8>();
	// SAFETY: returned length and alignment cover the complete SDK structure.
	let label = unsafe { (*base.cast::<TOKEN_MANDATORY_LABEL>()).Label };
	let sid_offset = label
		.Sid
		.addr()
		.checked_sub(base.addr())
		.filter(|&offset| (minimum..used).contains(&offset))
		.ok_or(StorageError::UnsafeMetadata(
			"token integrity SID lies outside its buffer",
		))?;
	// SAFETY: the SID offset and remaining byte count are both inside the token buffer.
	let sid = unsafe { slice::from_raw_parts(base.add(sid_offset), used - sid_offset) };
	let rid = integrity_rid(sid)?;
	let mut policy = TOKEN_MANDATORY_POLICY::default();
	let mut used = 0;
	let size = size_of::<TOKEN_MANDATORY_POLICY>() as u32;
	// SAFETY: this class writes one SDK policy structure to the live output slot.
	win32("query access-probe token mandatory policy", unsafe {
		GetTokenInformation(
			token.0.as_raw_handle(),
			TokenMandatoryPolicy,
			(&raw mut policy).cast(),
			size,
			&mut used,
		)
	})?;
	if used != size {
		return Err(StorageError::UnsafeMetadata(
			"invalid token mandatory policy record length",
		));
	}
	Ok((rid, label.Attributes, policy.Policy))
}

fn directory_label(file: &File) -> Result<Option<(u32, u32)>, StorageError> {
	let mut descriptor = ptr::null_mut();
	// SAFETY: the verified directory handle has READ_CONTROL. The API returns
	// a LocalFree-owned descriptor containing the requested label information only.
	let code = unsafe {
		GetSecurityInfo(
			file.as_raw_handle(),
			SE_FILE_OBJECT,
			LABEL_SECURITY_INFORMATION,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			&mut descriptor,
		)
	};
	if code != ERROR_SUCCESS {
		return Err(StorageError::io(
			"read directory mandatory security label",
			io::Error::from_raw_os_error(code.cast_signed()),
		));
	}
	let allocation = LocalAllocation::new(descriptor)?;
	let (mut present, mut defaulted, mut acl) = (0, 0, ptr::null_mut());
	// SAFETY: the returned security descriptor remains live through the ACL walk.
	win32("query directory mandatory label ACL", unsafe {
		GetSecurityDescriptorSacl(allocation.as_ptr(), &mut present, &mut acl, &mut defaulted)
	})?;
	if present == 0 || acl.is_null() {
		return Ok(None);
	}
	// SAFETY: the ACL pointer is obtained from a live descriptor owned by this function.
	if unsafe { IsValidAcl(acl) } == 0 {
		return Err(StorageError::UnsafeMetadata("invalid mandatory label ACL"));
	}
	// SAFETY: IsValidAcl confirmed the header and complete entries.
	let count = unsafe { (*acl).AceCount };
	if count > MAX_OBSERVED_ACES {
		return Err(StorageError::UnsafeMetadata(
			"mandatory label ACL exceeds its limit",
		));
	}
	let mut label = None;
	for index in 0..u32::from(count) {
		let mut entry = ptr::null_mut();
		// SAFETY: index is inside the verified live ACL and the output slot is writable.
		win32("query directory mandatory label ACE", unsafe {
			GetAce(acl, index, &mut entry)
		})?;
		if entry.is_null() {
			return Err(StorageError::UnsafeMetadata("missing mandatory label ACE"));
		}
		// SAFETY: GetAce returned a valid entry in the verified ACL.
		let header = unsafe { entry.cast::<ACE_HEADER>().read_unaligned() };
		if header.AceType != MANDATORY_LABEL_ACE_TYPE
			|| u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0
		{
			continue;
		}
		let sid_offset = offset_of!(SYSTEM_MANDATORY_LABEL_ACE, SidStart);
		let size = usize::from(header.AceSize);
		if size < size_of::<SYSTEM_MANDATORY_LABEL_ACE>() || label.is_some() {
			return Err(StorageError::UnsafeMetadata(
				"invalid effective mandatory label ACE",
			));
		}
		// SAFETY: the checked ACE size contains the SDK mask and the entire SID slice.
		let mask = unsafe { entry.cast::<SYSTEM_MANDATORY_LABEL_ACE>().read_unaligned() }.Mask;
		let sid =
			unsafe { slice::from_raw_parts(entry.cast::<u8>().add(sid_offset), size - sid_offset) };
		label = Some((integrity_rid(sid)?, mask));
	}
	Ok(label)
}

fn integrity_rid(sid: &[u8]) -> Result<u32, StorageError> {
	let Some(rid) = sid
		.strip_prefix(&INTEGRITY_SID_HEADER)
		.and_then(|tail| tail.first_chunk())
	else {
		return Err(StorageError::UnsafeMetadata(
			"invalid integrity authority SID",
		));
	};
	Ok(u32::from_le_bytes(*rid))
}
