//! Synthetic descriptor proof only: no cross-user access or namespace durability claim.
//!
//! Uses creation-time security, never a post-creation ACL repair:
//! https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntcreatefile
//! https://learn.microsoft.com/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Write};
use std::mem::size_of;
use std::os::windows::{
	ffi::OsStrExt,
	io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use std::path::Path;
use std::{ptr, slice};

use windows_sys::{
	Wdk::{
		Foundation::OBJECT_ATTRIBUTES,
		Storage::FileSystem::{
			FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_NO_RECALL,
			FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
		},
	},
	Win32::{
		Foundation::{
			ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, HANDLE,
			LocalFree, STATUS_SUCCESS, UNICODE_STRING,
		},
		Security::{
			ACCESS_ALLOWED_ACE,
			Authorization::{
				ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
				GetSecurityInfo, SE_FILE_OBJECT,
			},
			DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetAce,
			GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetSecurityDescriptorGroup,
			GetSecurityDescriptorOwner, GetTokenInformation, INHERITED_ACE,
			OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_AUTO_INHERIT_REQ,
			SE_DACL_AUTO_INHERITED, SE_DACL_DEFAULTED, SE_DACL_PRESENT, SE_DACL_PROTECTED,
			SE_GROUP_DEFAULTED, SE_OWNER_DEFAULTED, SE_SELF_RELATIVE, TOKEN_ELEVATION, TOKEN_OWNER,
			TOKEN_PRIMARY_GROUP, TOKEN_QUERY, TOKEN_USER, TokenElevation,
			TokenOwner as OWNER_CLASS, TokenPrimaryGroup as PRIMARY_GROUP_CLASS,
			TokenUser as USER_CLASS,
		},
		Storage::FileSystem::{
			DELETE, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
			FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
			FILE_TRAVERSE, READ_CONTROL, SYNCHRONIZE,
		},
		System::{
			IO::IO_STATUS_BLOCK,
			Threading::{GetCurrentProcess, OpenProcessToken},
			WindowsProgramming::FILE_CREATED,
		},
	},
};

use super::{
	DESTINATION_NAME, DirectoryScope, Observation, PreparedStage, STAGED, STAGED_NAME,
	assert_absent, verify_published,
};

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
	let (user, group, default_owner) = token_identity()?;
	// An inheritable Everyone read ACE deliberately makes the inherited control broader.
	// Protected owner-only security is supplied to NtCreateFile before any stage exists.
	let parent_descriptor = descriptor(&format!(
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
		FILE_ATTRIBUTE_NORMAL,
		FILE_DIRECTORY_FILE,
		parent_descriptor.0,
	)?;
	drop(directory);
	let path = root.join(SECURITY_DIRECTORY);
	let scope = DirectoryScope::open_parent(&path)?;
	let control = create_relative(
		&scope.parent,
		OsStr::new(INHERITED_CONTROL),
		GENERIC_READ | GENERIC_WRITE | READ_CONTROL | SYNCHRONIZE,
		FILE_ATTRIBUTE_ARCHIVE,
		FILE_NON_DIRECTORY_FILE,
		ptr::null_mut(),
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
	assert_ne!(inherited.control & SE_DACL_AUTO_INHERITED, 0);
	drop(control);

	let mut file = create_relative(
		&scope.parent,
		OsStr::new(STAGED_NAME),
		GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL | SYNCHRONIZE,
		FILE_ATTRIBUTE_ARCHIVE,
		FILE_NON_DIRECTORY_FILE,
		private_descriptor.0,
	)?;
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
	assert_eq!(SecurityObservation::read(&file)?, expected);
	let stage = PreparedStage {
		parent: &scope,
		file,
	};
	let status = stage.publish_status(&scope, OsStr::new(DESTINATION_NAME))?;
	assert_eq!(
		status, STATUS_SUCCESS,
		"private-stage rename NTSTATUS {status:#010x}"
	);
	assert_eq!(SecurityObservation::read(&stage.file)?, expected);
	assert_eq!(
		Observation::read(&stage.file)?.identity,
		stage_observation.identity
	);
	drop(stage);
	let published = scope.open_file(OsStr::new(DESTINATION_NAME))?;
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

fn token_identity() -> io::Result<(String, String, String)> {
	let mut token = ptr::null_mut();
	// SAFETY: current-process pseudo handle is valid, token output is writable. TOKEN_QUERY
	// neither requests nor enables privileges; OwnedHandle closes the successful token handle.
	win32(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
	// SAFETY: successful OpenProcessToken transferred one owned handle.
	let token = unsafe { OwnedHandle::from_raw_handle(token) };
	let user = token_sid(&token, USER_CLASS)?;
	let group = token_sid(&token, PRIMARY_GROUP_CLASS)?;
	let default_owner = token_sid(&token, OWNER_CLASS)?;
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
		"Private-stage probe token elevated={}, default owner differs from TokenUser={}; probe does not enable privileges",
		elevation.TokenIsElevated != 0,
		default_owner != user
	);
	Ok((user, group, default_owner))
}

fn token_sid(token: &OwnedHandle, class: i32) -> io::Result<String> {
	let mut required = 0;
	// SAFETY: a zero-length query only obtains the required buffer size.
	let result = unsafe {
		GetTokenInformation(
			token.as_raw_handle(),
			class,
			ptr::null_mut(),
			0,
			&mut required,
		)
	};
	let error = io::Error::last_os_error();
	if result != 0 || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
		return Err(error);
	}
	let minimum = match class {
		USER_CLASS => size_of::<TOKEN_USER>(),
		PRIMARY_GROUP_CLASS => size_of::<TOKEN_PRIMARY_GROUP>(),
		OWNER_CLASS => size_of::<TOKEN_OWNER>(),
		_ => unreachable!("only user, owner and primary group are queried"),
	};
	assert!(required as usize >= minimum);
	let mut buffer = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
	// SAFETY: usize allocation aligns both requested SDK structures and is at least required
	// bytes. Returned embedded SID stays live while sid_string copies its string representation.
	unsafe {
		win32(GetTokenInformation(
			token.as_raw_handle(),
			class,
			buffer.as_mut_ptr().cast(),
			required,
			&mut required,
		))?;
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

fn create_relative(
	parent: &File,
	leaf: &OsStr,
	access: u32,
	attributes: u32,
	kind: u32,
	descriptor: PSECURITY_DESCRIPTOR,
) -> io::Result<File> {
	super::super::check_component(leaf).map_err(io::Error::other)?;
	let mut name: Vec<u16> = leaf.encode_wide().collect();
	let length = u16::try_from(name.len() * size_of::<u16>()).map_err(io::Error::other)?;
	let mut name = UNICODE_STRING {
		Length: length,
		MaximumLength: length,
		Buffer: name.as_mut_ptr(),
	};
	let object = OBJECT_ATTRIBUTES {
		Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
		RootDirectory: parent.as_raw_handle(),
		ObjectName: &mut name,
		Attributes: 0,
		SecurityDescriptor: descriptor.cast(),
		SecurityQualityOfService: ptr::null_mut(),
	};
	let (share, flags) = if kind == FILE_DIRECTORY_FILE {
		(FILE_SHARE_READ | FILE_SHARE_WRITE, kind)
	} else {
		(
			FILE_SHARE_READ,
			kind | FILE_OPEN_REPARSE_POINT | FILE_OPEN_NO_RECALL,
		)
	};
	let options = flags | FILE_SYNCHRONOUS_IO_NONALERT;
	let mut file: HANDLE = ptr::null_mut();
	let mut completion = IO_STATUS_BLOCK::default();
	// SAFETY: held parent, single checked leaf, live descriptor and initialized SDK buffers.
	// FILE_CREATE never opens an existing object; synchronous completion owns exactly the
	// returned file handle. The directory fixture uses a fresh held temporary parent and
	// directory-compatible options. Regular files retain the reparse and recall guards.
	let status = unsafe {
		NtCreateFile(
			&mut file,
			access,
			&object,
			&mut completion,
			ptr::null(),
			attributes,
			share,
			FILE_CREATE,
			options,
			ptr::null(),
			0,
		)
	};
	if status != STATUS_SUCCESS {
		return Err(io::Error::other(format!(
			"NtCreateFile for fixture {leaf:?} with options {options:#010x} returned NTSTATUS {status:#010x}"
		)));
	}
	// SAFETY: successful exclusive NtCreateFile transfers the new owned handle to File.
	let file = unsafe { File::from_raw_handle(file) };
	assert_eq!(
		completion.Information, FILE_CREATED as usize,
		"FILE_CREATE must report a newly created object"
	);
	Ok(file)
}

fn win32(result: i32) -> io::Result<()> {
	if result == 0 {
		Err(io::Error::last_os_error())
	} else {
		Ok(())
	}
}
