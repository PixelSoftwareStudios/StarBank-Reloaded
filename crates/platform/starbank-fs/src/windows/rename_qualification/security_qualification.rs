//! Synthetic descriptor and same-user restricted-token proof. Cross-user access and namespace
//! durability remain separate tests.
//!
//! The private stage receives its descriptor at creation time:
//! https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntcreatefile
//! https://learn.microsoft.com/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Write};
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::path::Path;
use std::{ptr, slice};

use windows_sys::Win32::{
	Foundation::{ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, LocalFree, STATUS_SUCCESS},
	Security::{
		ACCESS_ALLOWED_ACE,
		Authorization::{
			ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
			GetSecurityInfo, SE_FILE_OBJECT,
		},
		DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetAce,
		GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorGroup,
		GetSecurityDescriptorOwner, GetTokenInformation, INHERITED_ACE, OWNER_SECURITY_INFORMATION,
		PSECURITY_DESCRIPTOR, PSID, SE_DACL_AUTO_INHERIT_REQ, SE_DACL_AUTO_INHERITED,
		SE_DACL_DEFAULTED, SE_DACL_PRESENT, SE_DACL_PROTECTED, SE_GROUP_DEFAULTED,
		SE_OWNER_DEFAULTED, SE_SELF_RELATIVE, TOKEN_ELEVATION, TOKEN_OWNER, TOKEN_PRIMARY_GROUP,
		TOKEN_USER, TokenElevation, TokenOwner as OWNER_CLASS,
		TokenPrimaryGroup as PRIMARY_GROUP_CLASS, TokenUser as USER_CLASS,
	},
	Storage::FileSystem::{
		FILE_ATTRIBUTE_ARCHIVE, FILE_GENERIC_READ, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
		FILE_TRAVERSE, READ_CONTROL, SYNCHRONIZE,
	},
};

use crate::windows::stage_security::{
	EffectiveToken, PrivateStageSecurity, RelativeKind, SecurityDescriptor, create_relative,
};

use super::{
	DESTINATION_NAME, DirectoryScope, Observation, PreparedStage, STAGED, STAGED_NAME,
	assert_absent, verify_published,
};

#[path = "security_qualification/fixture_acl.rs"]
mod fixture_acl;
#[path = "security_qualification/restricted_read.rs"]
mod restricted_read;
#[path = "security_qualification/restricted_token.rs"]
mod restricted_token;

pub(in crate::windows) use fixture_acl::prepare_reduced_fixture;
use restricted_read::{CONTROL_BYTES, qualify_restricted_read};
pub(in crate::windows) use restricted_token::with_reduced_token;
use restricted_token::{Actors, TokenInformation};

const SECURITY_DIRECTORY: &str = "PrivateStageSecurity";
const INHERITED_CONTROL: &str = "InheritedControl";
const SECURITY_STRING_REVISION: u32 = 1;
const MAX_SID_STRING_UNITS: usize = 184;
const OWNER_GROUP_DACL: u32 =
	OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
// Audit/SACL fields are deliberately outside the queried and compared security information.
const CONTROL_MASK: u16 = SE_OWNER_DEFAULTED
	| SE_GROUP_DEFAULTED
	| SE_DACL_PRESENT
	| SE_DACL_DEFAULTED
	| SE_DACL_AUTO_INHERIT_REQ
	| SE_DACL_AUTO_INHERITED
	| SE_DACL_PROTECTED
	| SE_SELF_RELATIVE;
const ACCESS_ALLOWED_ACE_TAG: u8 = 0; // Windows SDK ACCESS_ALLOWED_ACE_TYPE.
const WORLD_SID: &str = "S-1-1-0";
const PRIVATE_CONTROL: u16 = SE_DACL_PRESENT | SE_DACL_PROTECTED | SE_SELF_RELATIVE;

struct LocalAllocation(*mut core::ffi::c_void);

impl Drop for LocalAllocation {
	fn drop(&mut self) {
		// SAFETY: these allocations are returned by documented LocalFree-owning Win32 APIs.
		unsafe {
			LocalFree(self.0);
		}
	}
}
#[derive(PartialEq, Eq)]
struct AllowedAce {
	flags: u8,
	mask: u32,
	sid: String,
}
#[derive(PartialEq, Eq)]
struct SecurityObservation {
	owner: String,
	group: String,
	dacl: Vec<AllowedAce>,
	control: u16,
}
impl std::fmt::Debug for SecurityObservation {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SecurityObservation")
			.field("identities_and_aces", &"redacted")
			.field("ace_count", &self.dacl.len())
			.field("control", &self.control)
			.finish()
	}
}

impl SecurityObservation {
	fn from_descriptor(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<Self> {
		let (mut owner, mut group, mut dacl) = (ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
		let (mut defaulted, mut present, mut control, mut revision) = (0, 0, 0, 0);
		// SAFETY: descriptor is a live OS-allocated descriptor; all output pointers are writable.
		unsafe {
			win32(GetSecurityDescriptorOwner(
				descriptor,
				&mut owner,
				&mut defaulted,
			))?;
			win32(GetSecurityDescriptorGroup(
				descriptor,
				&mut group,
				&mut defaulted,
			))?;
			win32(GetSecurityDescriptorDacl(
				descriptor,
				&mut present,
				&mut dacl,
				&mut defaulted,
			))?;
			win32(GetSecurityDescriptorControl(
				descriptor,
				&mut control,
				&mut revision,
			))?;
		}
		assert_ne!(present, 0, "a null or absent DACL must never qualify");
		assert!(!dacl.is_null(), "a null DACL grants unrestricted access");
		let mut entries = Vec::new();
		// SAFETY: successful descriptor query returns a valid ACL and bounded ACE count.
		for index in 0..unsafe { (*dacl).AceCount } {
			let mut entry = ptr::null_mut();
			// SAFETY: GetAce validates this index in the live descriptor's ACL.
			win32(unsafe { GetAce(dacl, u32::from(index), &mut entry) })?;
			// SAFETY: every ACE has an ACE_HEADER. Only the concrete allowed layout is read.
			let header = unsafe { &*entry.cast::<windows_sys::Win32::Security::ACE_HEADER>() };
			assert_eq!(
				header.AceType, ACCESS_ALLOWED_ACE_TAG,
				"unexpected ACE type"
			);
			assert!(usize::from(header.AceSize) >= size_of::<ACCESS_ALLOWED_ACE>());
			let entry = entry.cast::<ACCESS_ALLOWED_ACE>();
			// SAFETY: GetAce provided a validated allowed ACE, including its variable-length SID.
			let (mask, sid) = unsafe {
				(
					(*entry).Mask,
					sid_string((&raw mut (*entry).SidStart).cast())?,
				)
			};
			entries.push(AllowedAce {
				flags: header.AceFlags,
				mask,
				sid,
			});
		}

		Ok(Self {
			owner: sid_string(owner)?,
			group: sid_string(group)?,
			dacl: entries,
			control: control & CONTROL_MASK,
		})
	}

	fn read(file: &File) -> io::Result<Self> {
		let mut descriptor = ptr::null_mut();
		// SAFETY: file remains open with READ_CONTROL. Only owner/group/DACL are requested;
		// the returned descriptor remains allocated until the observation has copied them.
		let error = unsafe {
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
		if error != ERROR_SUCCESS {
			return Err(io::Error::from_raw_os_error(error as i32));
		}
		let allocation = LocalAllocation(descriptor);
		Self::from_descriptor(allocation.0)
	}
}

pub(super) fn qualify_private_stage(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
	let actors = Actors::for_current_user()?;
	actors.as_creator(|effective| qualify_scoped(root, effective, &actors))?;
	Ok(())
}

fn qualify_scoped(
	root: &Path,
	token: &OwnedHandle,
	actors: &Actors,
) -> Result<(), Box<dyn std::error::Error>> {
	let (user, group, default_owner) = token_identity(token)?;
	let selected = EffectiveToken::current()?;
	assert!(
		selected.default_owner()? == default_owner,
		"effective default owner"
	);
	assert_eq!(
		selected.selected_type()?,
		windows_sys::Win32::Security::TokenImpersonation,
	);
	let security = PrivateStageSecurity::current()?;
	// An inheritable Everyone read ACE deliberately makes the inherited control broader.
	// Protected owner-only security is supplied to NtCreateFile before any stage exists.
	let parent_descriptor = SecurityDescriptor::from_sddl(&format!(
		"O:{user}G:{group}D:P(A;OICI;FA;;;{user})(A;OICI;FR;;;WD)"
	))?;
	let private_descriptor = descriptor(&format!("O:{user}G:{group}D:P(A;;FA;;;{user})"))?;
	let expected = SecurityObservation::from_descriptor(private_descriptor.0)?;
	assert!(expected.owner == user, "explicit owner must be TokenUser");
	assert!(
		expected.group == group,
		"explicit group must be TokenPrimaryGroup"
	);
	assert_eq!(expected.control, PRIVATE_CONTROL);
	let outer = DirectoryScope::open_parent(root)?;
	let directory = create_relative(
		&outer.parent,
		OsStr::new(SECURITY_DIRECTORY),
		FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
		RelativeKind::Directory,
		Some(&parent_descriptor),
	)?;
	drop(directory);
	let path = root.join(SECURITY_DIRECTORY);
	let scope = DirectoryScope::open_parent(&path)?;
	let mut control = create_relative(
		&scope.parent,
		OsStr::new(INHERITED_CONTROL),
		GENERIC_READ | GENERIC_WRITE | READ_CONTROL | SYNCHRONIZE,
		RelativeKind::File,
		None,
	)?;
	let inherited = SecurityObservation::read(&control)?;
	assert!(
		inherited.owner == default_owner,
		"implicit control owner must match TokenOwner"
	);
	assert!(
		inherited.group == expected.group,
		"control primary group must match"
	);
	assert!(
		inherited.dacl.iter().any(|ace| ace.sid == WORLD_SID
			&& ace.mask == FILE_GENERIC_READ
			&& u32::from(ace.flags) & INHERITED_ACE != 0),
		"control must contain an inherited Everyone file-read ACE"
	);
	assert_eq!(inherited.control & SE_DACL_PROTECTED, 0);
	eprintln!("Inherited control descriptor: {inherited:?}");
	control.write_all(CONTROL_BYTES)?;
	control.sync_all()?;
	drop(control);

	assert!(matches!(
		security.verify(&scope.open_file(OsStr::new(INHERITED_CONTROL))?),
		Err(crate::StorageError::UnsafeMetadata(_))
	));
	let occupied_before = Observation::read(&scope.open_file(OsStr::new(INHERITED_CONTROL))?)?;
	let collision = security
		.create(&scope, OsStr::new(INHERITED_CONTROL))
		.expect_err("exclusive creation must preserve the inherited control");
	assert_eq!(collision.kind(), io::ErrorKind::AlreadyExists);
	assert_eq!(
		Observation::read(&scope.open_file(OsStr::new(INHERITED_CONTROL))?)?,
		occupied_before
	);
	assert_eq!(std::fs::read(path.join(INHERITED_CONTROL))?, CONTROL_BYTES);
	let mut file = security.create(&scope, OsStr::new(STAGED_NAME))?;
	security.verify(&file)?;
	assert_eq!(
		SecurityObservation::read(&file)?,
		expected,
		"stage must be private before its first byte is written"
	);
	let stage_observation = Observation::read(&file)?;
	assert_eq!(stage_observation.attributes, FILE_ATTRIBUTE_ARCHIVE);
	assert_eq!(stage_observation.len, 0);
	file.write_all(STAGED)?;
	file.sync_all()?;
	security.verify(&file)?;
	assert_eq!(SecurityObservation::read(&file)?, expected);
	qualify_restricted_read(&scope.parent, actors)?;
	let stage = PreparedStage {
		parent: &scope,
		file,
	};
	let status = stage.publish_status(&scope, OsStr::new(DESTINATION_NAME))?;
	assert_eq!(
		status, STATUS_SUCCESS,
		"private-stage rename NTSTATUS {status:#010x}"
	);
	security.verify(&stage.file)?;
	assert_eq!(SecurityObservation::read(&stage.file)?, expected);
	assert_eq!(
		Observation::read(&stage.file)?.identity,
		stage_observation.identity
	);
	drop(stage);
	let published = scope.open_file(OsStr::new(DESTINATION_NAME))?;
	security.verify(&published)?;
	assert_eq!(
		SecurityObservation::read(&published)?,
		expected,
		"fresh published handle must retain exact owner/group/DACL and scoped flags"
	);
	let observed = Observation::read(&published)?;
	assert_eq!(observed.identity, stage_observation.identity);
	assert_eq!(observed.attributes, FILE_ATTRIBUTE_ARCHIVE);
	assert_eq!(observed.len, STAGED.len() as u64);
	drop(published);
	verify_published(&path, &scope, stage_observation.identity)?;
	assert_absent(&path.join(STAGED_NAME));
	eprintln!(
		"Private NTFS stage qualified: exact token-user owner, primary group, protected owner-only DACL, scoped control {:#06x}, attributes {:#010x}; inherited control differed. Cross-user denial, SACL and namespace durability untested.",
		expected.control, observed.attributes
	);
	Ok(())
}

fn descriptor(sddl: &str) -> io::Result<LocalAllocation> {
	let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
	let mut result = ptr::null_mut();
	// SAFETY: NUL-terminated SDDL and output pointers remain live for the synchronous call.
	win32(unsafe {
		ConvertStringSecurityDescriptorToSecurityDescriptorW(
			sddl.as_ptr(),
			SECURITY_STRING_REVISION,
			&mut result,
			ptr::null_mut(),
		)
	})?;
	Ok(LocalAllocation(result))
}

fn token_identity(token: &OwnedHandle) -> io::Result<(String, String, String)> {
	let user = token_sid(token, USER_CLASS)?;
	let group = token_sid(token, PRIMARY_GROUP_CLASS)?;
	let default_owner = token_sid(token, OWNER_CLASS)?;
	let mut elevation = TOKEN_ELEVATION::default();
	let mut returned = 0;
	// SAFETY: fixed SDK structure and exact length match the requested information class.
	win32(unsafe {
		GetTokenInformation(
			token.as_raw_handle(),
			TokenElevation,
			(&raw mut elevation).cast(),
			size_of::<TOKEN_ELEVATION>() as u32,
			&mut returned,
		)
	})?;
	assert_eq!(returned as usize, size_of::<TOKEN_ELEVATION>());
	eprintln!(
		"Effective private-stage probe token elevated={}, default owner differs from TokenUser={}; probe does not enable privileges",
		elevation.TokenIsElevated != 0,
		default_owner != user
	);
	Ok((user, group, default_owner))
}

fn token_sid(token: &OwnedHandle, class: i32) -> io::Result<String> {
	let minimum = match class {
		USER_CLASS => size_of::<TOKEN_USER>(),
		PRIMARY_GROUP_CLASS => size_of::<TOKEN_PRIMARY_GROUP>(),
		OWNER_CLASS => size_of::<TOKEN_OWNER>(),
		_ => unreachable!("only user, owner and primary group are queried"),
	};
	let buffer = TokenInformation::query(token, class, minimum)?;
	// SAFETY: the pointer-aligned token buffer covers the selected SDK structure. Any
	// embedded SID remains live while sid_string copies its representation.
	unsafe {
		let sid = match class {
			USER_CLASS => (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid,
			PRIMARY_GROUP_CLASS => (*buffer.as_ptr().cast::<TOKEN_PRIMARY_GROUP>()).PrimaryGroup,
			OWNER_CLASS => (*buffer.as_ptr().cast::<TOKEN_OWNER>()).Owner,
			_ => unreachable!("only user, owner and primary group are queried"),
		};
		sid_string(sid)
	}
}

fn sid_string(sid: PSID) -> io::Result<String> {
	let mut output = ptr::null_mut();
	// SAFETY: SID comes from a live successful token/descriptor query. Output allocation is
	// returned NUL-terminated by the API and released with LocalFree after copying.
	win32(unsafe { ConvertSidToStringSidW(sid, &mut output) })?;
	let allocation = LocalAllocation(output.cast());
	for length in 0..MAX_SID_STRING_UNITS {
		// SAFETY: stop at the first terminator in the OS-allocated SID string.
		if unsafe { *output.add(length) } == 0 {
			let units = unsafe { slice::from_raw_parts(output, length) };
			let result = String::from_utf16(units).map_err(io::Error::other);
			drop(allocation);
			return result;
		}
	}
	Err(io::Error::other(
		"SID string exceeds documented bounded representation",
	))
}

fn win32(result: i32) -> io::Result<()> {
	if result == 0 {
		Err(io::Error::last_os_error())
	} else {
		Ok(())
	}
}
