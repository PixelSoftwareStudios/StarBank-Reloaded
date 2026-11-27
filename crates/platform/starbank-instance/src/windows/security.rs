//! Native access checks and handle ownership for the Windows launch pipe.
#![allow(unsafe_code, reason = "Windows logon SID and named-pipe security APIs")]

use std::{
	fmt,
	fs::OpenOptions,
	io,
	mem::{offset_of, size_of},
	os::windows::{
		fs::OpenOptionsExt as _,
		io::{AsRawHandle as _, FromRawHandle as _, IntoRawHandle as _, OwnedHandle},
	},
	ptr::{self, NonNull},
};

use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer, ServerOptions};
use windows_sys::{
	Win32::{
		Foundation::{HANDLE, LocalFree, WAIT_FAILED, WAIT_TIMEOUT},
		Security::{
			Authorization::{
				ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
			},
			GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE,
			TOKEN_GROUPS, TOKEN_QUERY, TokenLogonSid,
		},
		Storage::FileSystem::{
			FILE_FLAG_OVERLAPPED, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_TYPE_PIPE,
			FILE_WRITE_DATA, GetFileType, READ_CONTROL, SECURITY_IDENTIFICATION, SYNCHRONIZE,
		},
		System::{
			Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
			Threading::{
				GetCurrentProcess, OpenProcess, OpenProcessToken,
				PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
			},
		},
	},
	core::BOOL,
};

use crate::windows::Namespace;

const PIPE_PREFIX: &str = r"\\.\pipe\StarBankReloaded.v1";
// FILE_APPEND_DATA aliases FILE_CREATE_PIPE_INSTANCE and is deliberately outside this mask.
const CLIENT_ACCESS: u32 =
	FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE;
const PIPE_KERNEL_BUFFER_BYTES: u32 = 4096;
const LOGON_SID_PREFIX: [u8; 12] = [1, 3, 0, 0, 0, 0, 0, 5, 5, 0, 0, 0];
const LOGON_SID_BYTES: usize = LOGON_SID_PREFIX.len() + 2 * size_of::<u32>();

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct LogonSid([u32; 2]);

impl LogonSid {
	pub(crate) fn current() -> io::Result<Self> {
		// SAFETY: GetCurrentProcess returns a borrowed pseudohandle valid for this process.
		Self::read(unsafe { GetCurrentProcess() })
	}

	pub(crate) fn endpoint(self, namespace: Namespace<'_>) -> Address {
		Address(format!("{PIPE_PREFIX}.{}.{self}", namespace.as_str()))
	}

	pub(crate) fn verify_server(self, pipe: &NamedPipeClient) -> io::Result<VerifiedPeer> {
		self.verify_peer(pipe.as_raw_handle(), GetNamedPipeServerProcessId)
	}

	pub(crate) fn verify_client(self, pipe: &NamedPipeServer) -> io::Result<VerifiedPeer> {
		self.verify_peer(pipe.as_raw_handle(), GetNamedPipeClientProcessId)
	}

	fn verify_peer(
		self,
		pipe: HANDLE,
		query: unsafe extern "system" fn(HANDLE, *mut u32) -> BOOL,
	) -> io::Result<VerifiedPeer> {
		let mut pid = 0;
		// SAFETY: callers hold the connected pipe endpoint and query matches its peer side.
		// The output holds one writable Win32 process ID.
		if unsafe { query(pipe, &mut pid) } == 0 {
			return Err(io::Error::last_os_error());
		}
		// SAFETY: the only inputs are query-validated PID, a fixed access mask and no inheritance.
		let peer = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
		if peer.is_null() {
			return Err(io::Error::last_os_error());
		}
		// SAFETY: successful OpenProcess gives this caller exclusive ownership of the handle.
		let peer = unsafe { OwnedHandle::from_raw_handle(peer) };
		if Self::read(peer.as_raw_handle())? != self {
			return Err(io::Error::new(
				io::ErrorKind::PermissionDenied,
				"instance peer has a different logon SID",
			));
		}
		// SAFETY: the live process handle has SYNCHRONIZE and wait zero never blocks.
		match unsafe { WaitForSingleObject(peer.as_raw_handle(), 0) } {
			WAIT_TIMEOUT => {}
			WAIT_FAILED => return Err(io::Error::last_os_error()),
			_ => {
				return Err(io::Error::new(
					io::ErrorKind::NotConnected,
					"instance peer exited",
				));
			}
		}
		let mut current_pid = 0;
		// SAFETY: same live connected endpoint and one writable process ID as above.
		if unsafe { query(pipe, &mut current_pid) } == 0 {
			return Err(io::Error::last_os_error());
		}
		if current_pid != pid {
			return Err(io::Error::new(
				io::ErrorKind::PermissionDenied,
				"instance pipe peer changed",
			));
		}
		Ok(VerifiedPeer {
			_process: peer,
			#[cfg(test)]
			pid,
		})
	}

	fn read(process: HANDLE) -> io::Result<Self> {
		let mut token: HANDLE = ptr::null_mut();
		// SAFETY: callers hold a live process handle (or the current pseudohandle); the call
		// writes a token handle only into the initialized output slot.
		if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
			return Err(io::Error::last_os_error());
		}
		// SAFETY: successful OpenProcessToken transfers ownership of the returned token.
		let token = unsafe { OwnedHandle::from_raw_handle(token) };
		let mut information = TokenBuffer {
			groups: TOKEN_GROUPS::default(),
			sid: [0; SECURITY_MAX_SID_SIZE as usize],
		};
		let mut length = 0;
		let information_ptr = &raw mut information;
		let capacity = u32::try_from(size_of::<TokenBuffer>()).map_err(io::Error::other)?;
		// SAFETY: the live query token and initialized, naturally aligned buffer with its exact
		// capacity remain available through the synchronous query. Returned pointers are checked
		// against that allocation before reading them.
		if unsafe {
			GetTokenInformation(
				token.as_raw_handle(),
				TokenLogonSid,
				information_ptr.cast(),
				capacity,
				&mut length,
			)
		} == 0
		{
			return Err(io::Error::last_os_error());
		}
		let length = usize::try_from(length).map_err(io::Error::other)?;
		if !(size_of::<TOKEN_GROUPS>()..=size_of::<TokenBuffer>()).contains(&length)
			|| information.groups.GroupCount != 1
		{
			return Err(invalid_logon_sid());
		}
		let offset = information.groups.Groups[0]
			.Sid
			.addr()
			.checked_sub(information_ptr.addr())
			.ok_or_else(invalid_logon_sid)?;
		if !offset
			.checked_add(LOGON_SID_BYTES)
			.is_some_and(|end| end <= length)
		{
			return Err(invalid_logon_sid());
		}
		let index = offset
			.checked_sub(offset_of!(TokenBuffer, sid))
			.ok_or_else(invalid_logon_sid)?;
		let bytes = information
			.sid
			.get(index..index + LOGON_SID_BYTES)
			.ok_or_else(invalid_logon_sid)?;
		let sid = bytes.try_into().map_err(|_| invalid_logon_sid())?;
		Self::from_bytes(sid)
	}

	fn from_bytes(sid: [u8; LOGON_SID_BYTES]) -> io::Result<Self> {
		// A logon SID has revision 1 and the standard S-1-5-5-X-Y form. The two trailing
		// subauthorities are stored little-endian in the binary SID.
		let [prefix @ .., a, b, c, d, e, f, g, h] = sid;
		if prefix != LOGON_SID_PREFIX {
			return Err(invalid_logon_sid());
		}
		Ok(Self([
			u32::from_le_bytes([a, b, c, d]),
			u32::from_le_bytes([e, f, g, h]),
		]))
	}
}

impl fmt::Display for LogonSid {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "S-1-5-5-{}-{}", self.0[0], self.0[1])
	}
}

#[repr(C)]
struct TokenBuffer {
	groups: TOKEN_GROUPS,
	sid: [u8; SECURITY_MAX_SID_SIZE as usize],
}

pub(crate) struct Address(String);

pub(crate) struct VerifiedPeer {
	_process: OwnedHandle,
	#[cfg(test)]
	pid: u32,
}

#[cfg(test)]
impl VerifiedPeer {
	pub(crate) fn pid(&self) -> u32 {
		self.pid
	}
}

struct Descriptor(NonNull<std::ffi::c_void>);

impl Descriptor {
	fn for_logon(owner: &LogonSid) -> io::Result<Self> {
		let sddl: Vec<_> = format!("D:P(D;;WD;;;OW)(A;;{CLIENT_ACCESS:#x};;;{owner})")
			.encode_utf16()
			.chain([0])
			.collect();
		let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
		// SAFETY: SDDL is NUL-terminated and built only from the locally obtained numeric SID
		// and a fixed rights mask. The API writes into a valid pointer slot.
		if unsafe {
			ConvertStringSecurityDescriptorToSecurityDescriptorW(
				sddl.as_ptr(),
				SDDL_REVISION_1,
				&mut descriptor,
				ptr::null_mut(),
			)
		} == 0
		{
			return Err(io::Error::last_os_error());
		}
		NonNull::new(descriptor)
			.map(Self)
			.ok_or_else(|| io::Error::other("missing instance pipe security descriptor"))
	}
}

impl Drop for Descriptor {
	fn drop(&mut self) {
		// SAFETY: the descriptor was allocated by a Windows LocalAlloc-backed API and
		// Descriptor is its sole owner.
		unsafe { LocalFree(self.0.as_ptr()) };
	}
}

pub(crate) fn bind(address: &Address, owner: &LogonSid) -> io::Result<NamedPipeServer> {
	let descriptor = Descriptor::for_logon(owner)?;
	let mut attributes = SECURITY_ATTRIBUTES {
		nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).map_err(io::Error::other)?,
		lpSecurityDescriptor: descriptor.0.as_ptr(),
		bInheritHandle: 0,
	};
	let mut options = ServerOptions::new();
	options
		.first_pipe_instance(true)
		.max_instances(1)
		.reject_remote_clients(true)
		.in_buffer_size(PIPE_KERNEL_BUFFER_BYTES)
		.out_buffer_size(PIPE_KERNEL_BUFFER_BYTES);
	// SAFETY: SECURITY_ATTRIBUTES and its owned descriptor remain live until the synchronous
	// CreateNamedPipeW call consumes them. Tokio owns the resulting overlapped pipe handle.
	unsafe { options.create_with_security_attributes_raw(&address.0, (&raw mut attributes).cast()) }
}

pub(crate) fn connect(address: &Address) -> io::Result<NamedPipeClient> {
	let pipe = OpenOptions::new()
		.access_mode(CLIENT_ACCESS)
		.custom_flags(FILE_FLAG_OVERLAPPED)
		.security_qos_flags(SECURITY_IDENTIFICATION)
		.open(&address.0)?;
	// SAFETY: the owned client handle stays open while Windows queries its object type.
	if unsafe { GetFileType(pipe.as_raw_handle()) } != FILE_TYPE_PIPE {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"instance endpoint is not a pipe",
		));
	}
	// SAFETY: the validated endpoint is a local named pipe. OpenOptions created a single
	// overlapped client handle, now transferred exactly once to Tokio for its I/O lifetime.
	unsafe { NamedPipeClient::from_raw_handle(pipe.into_raw_handle()) }
}

fn invalid_logon_sid() -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, "invalid Windows logon SID")
}

#[cfg(test)]
pub(crate) mod qualification {
	use tokio::net::windows::named_pipe::PipeEnd;
	use windows_sys::{
		Wdk::Foundation::{NtQueryObject, ObjectBasicInformation},
		Win32::{
			Foundation::{ERROR_SUCCESS, STATUS_SUCCESS},
			Security::{
				ACCESS_ALLOWED_ACE, ACL,
				Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
				DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorControl, SE_DACL_PROTECTED,
			},
			Storage::FileSystem::{FILE_CREATE_PIPE_INSTANCE, WRITE_DAC},
			System::WindowsProgramming::PUBLIC_OBJECT_BASIC_INFORMATION,
		},
	};

	use super::*;

	const OWNER_RIGHTS_SID: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 3, 4, 0, 0, 0];
	const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
	const ACCESS_DENIED_ACE_TYPE: u8 = 1;

	pub(crate) fn verify_server_acl(pipe: &NamedPipeServer, owner: LogonSid) -> io::Result<()> {
		assert_eq!(pipe.info()?.max_instances, 1);
		let mut acl: *mut ACL = ptr::null_mut();
		let mut value: PSECURITY_DESCRIPTOR = ptr::null_mut();
		// SAFETY: the pipe remains open and the DACL and descriptor output slots are writable.
		let code = unsafe {
			GetSecurityInfo(
				pipe.as_raw_handle(),
				SE_FILE_OBJECT,
				DACL_SECURITY_INFORMATION,
				ptr::null_mut(),
				ptr::null_mut(),
				&mut acl,
				ptr::null_mut(),
				&mut value,
			)
		};
		if code != ERROR_SUCCESS {
			return Err(io::Error::from_raw_os_error(code.cast_signed()));
		}
		let descriptor = Descriptor(
			NonNull::new(value)
				.ok_or_else(|| io::Error::other("missing pipe test security descriptor"))?,
		);
		assert!(!acl.is_null());
		let mut control = 0;
		let mut revision = 0;
		// SAFETY: descriptor owns the live system security descriptor and both outputs are writable.
		if unsafe {
			GetSecurityDescriptorControl(descriptor.0.as_ptr(), &mut control, &mut revision)
		} == 0
		{
			return Err(io::Error::last_os_error());
		}
		assert_eq!(control & SE_DACL_PROTECTED, SE_DACL_PROTECTED);
		// SAFETY: GetSecurityInfo returned a non-null ACL belonging to the live descriptor.
		assert_eq!(unsafe { (*acl).AceCount }, 2);
		let mut denied = ptr::null_mut();
		let mut allowed = ptr::null_mut();
		// SAFETY: the live ACL contains two ACEs and the pointer slots are writable.
		if unsafe { GetAce(acl, 0, &mut denied) == 0 || GetAce(acl, 1, &mut allowed) == 0 } {
			return Err(io::Error::last_os_error());
		}
		// SAFETY: both ACE pointers belong to the live descriptor. The helper checks the exact
		// entry size before reading the common header, mask and SID for each access entry.
		let denied_sid = unsafe {
			read_access_ace::<{ OWNER_RIGHTS_SID.len() }>(denied, ACCESS_DENIED_ACE_TYPE, WRITE_DAC)
		};
		// SAFETY: the second checked access entry has the same lifetime as the descriptor.
		let allowed_sid = unsafe {
			read_access_ace::<LOGON_SID_BYTES>(allowed, ACCESS_ALLOWED_ACE_TYPE, CLIENT_ACCESS)
		};
		assert_eq!(denied_sid, OWNER_RIGHTS_SID);
		assert!(LogonSid::from_bytes(allowed_sid)? == owner);
		Ok(())
	}

	/// # Safety
	/// `ace` must be a live ACL entry returned by `GetAce` for the verified descriptor.
	unsafe fn read_access_ace<const SID_BYTES: usize>(
		ace: *mut std::ffi::c_void,
		ace_type: u8,
		mask: u32,
	) -> [u8; SID_BYTES] {
		use windows_sys::Win32::Security::ACE_HEADER;
		assert!(!ace.is_null());
		// SAFETY: a valid ACL entry always begins with ACE_HEADER, including its declared size.
		let header = unsafe { ace.cast::<ACE_HEADER>().read_unaligned() };
		assert_eq!(header.AceType, ace_type);
		let sid_offset = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
		assert_eq!(usize::from(header.AceSize), sid_offset + SID_BYTES);
		// SAFETY: matching ACE type and size establish the mask and SID field bounds.
		let actual_mask = unsafe {
			ace.cast::<u8>()
				.add(offset_of!(ACCESS_ALLOWED_ACE, Mask))
				.cast::<u32>()
				.read_unaligned()
		};
		assert_eq!(actual_mask, mask);
		// SAFETY: the checked size includes all SID bytes and the entry is still live.
		unsafe {
			ace.cast::<u8>()
				.add(sid_offset)
				.cast::<[u8; SID_BYTES]>()
				.read()
		}
	}

	pub(crate) fn verify_client_handle(pipe: &NamedPipeClient, owner: LogonSid) -> io::Result<()> {
		assert_eq!(pipe.info()?.end, PipeEnd::Client);
		let mut info = PUBLIC_OBJECT_BASIC_INFORMATION::default();
		let mut length = 0;
		let size = u32::try_from(size_of::<PUBLIC_OBJECT_BASIC_INFORMATION>())
			.map_err(io::Error::other)?;
		// SAFETY: the live endpoint and SDK-aligned object info output remain valid for the
		// synchronous query. ObjectBasicInformation returns the granted handle access mask.
		let status = unsafe {
			NtQueryObject(
				pipe.as_raw_handle(),
				ObjectBasicInformation,
				(&raw mut info).cast(),
				size,
				&mut length,
			)
		};
		assert_eq!(
			status, STATUS_SUCCESS,
			"object access query NTSTATUS {status:#010x}"
		);
		assert_eq!(length, size);
		assert_eq!(info.GrantedAccess & CLIENT_ACCESS, CLIENT_ACCESS);
		assert_eq!(
			info.GrantedAccess & (FILE_CREATE_PIPE_INSTANCE | WRITE_DAC),
			0
		);
		let _peer = owner.verify_server(pipe)?;
		let unexpected = LogonSid([owner.0[0] ^ 1, owner.0[1]]);
		assert!(
			matches!(unexpected.verify_server(pipe), Err(error) if error.kind() == io::ErrorKind::PermissionDenied)
		);
		Ok(())
	}

	pub(crate) fn verify_owner_cannot_change_acl(address: &Address) -> io::Result<()> {
		let result = OpenOptions::new()
			.access_mode(CLIENT_ACCESS | WRITE_DAC)
			.custom_flags(FILE_FLAG_OVERLAPPED)
			.security_qos_flags(SECURITY_IDENTIFICATION)
			.open(&address.0);
		assert_eq!(
			result
				.expect_err("pipe owner must not receive implicit WRITE_DAC")
				.kind(),
			io::ErrorKind::PermissionDenied
		);
		Ok(())
	}
}
