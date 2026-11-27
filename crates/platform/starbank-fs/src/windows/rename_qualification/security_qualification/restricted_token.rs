//! Test-only same-user access restrictions. Impersonation is local to one dedicated OS thread.

use std::{
	error::Error,
	io,
	marker::PhantomData,
	mem::{offset_of, size_of},
	os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
	ptr,
	rc::Rc,
	slice, thread,
};

use windows_sys::Win32::{
	Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_TOKEN, HANDLE, LUID},
	Security::{
		CheckTokenMembership, CreateRestrictedToken, CreateWellKnownSid, DISABLE_MAX_PRIVILEGE,
		EqualSid, GetTokenInformation, ImpersonateLoggedOnUser, IsTokenRestricted,
		LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, PSID, RevertToSelf, SE_CHANGE_NOTIFY_NAME,
		SE_PRIVILEGE_ENABLED, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES, TOKEN_DUPLICATE,
		TOKEN_GROUPS, TOKEN_PRIVILEGES, TOKEN_QUERY, TokenPrivileges, TokenRestrictedSids,
		WELL_KNOWN_SID_TYPE, WinBuiltinAdministratorsSid, WinWorldSid,
	},
	System::Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
};

use super::{USER_CLASS, token_sid, win32};

const TOKEN_INFORMATION_LIMIT: usize = 64 * 1024;
const SID_WORDS: usize = (SECURITY_MAX_SID_SIZE as usize).div_ceil(size_of::<u32>());

type FixtureResult = Result<(), Box<dyn Error>>;

pub(super) struct Actors {
	creator: OwnedHandle,
	reader: OwnedHandle,
	administrators: WellKnownSid,
	world: WellKnownSid,
	user: String,
	source_administrator: bool,
}

#[derive(Clone, Copy)]
enum Role {
	Creator,
	Reader,
}

impl Actors {
	pub(super) fn for_current_user() -> io::Result<Self> {
		let token = process_token(TOKEN_DUPLICATE | TOKEN_QUERY)?;
		if is_restricted(&token) {
			return Err(io::Error::other(
				"the source token already has restricting SIDs",
			));
		}
		let administrators = WellKnownSid::new(WinBuiltinAdministratorsSid)?;
		let world = WellKnownSid::new(WinWorldSid)?;
		let source_administrator = membership(ptr::null_mut(), &administrators)?;
		let creator = restrict(&token, &administrators, None)?;
		let reader = restrict(&token, &administrators, Some(&world))?;
		let user = token_sid(&token, USER_CLASS)?;
		Ok(Self {
			creator,
			reader,
			administrators,
			world,
			user,
			source_administrator,
		})
	}

	pub(super) fn as_creator(
		&self,
		action: impl FnOnce(&OwnedHandle) -> FixtureResult + Send,
	) -> io::Result<()> {
		self.on_thread(&self.creator, Role::Creator, action)
	}

	pub(super) fn as_reader(
		&self,
		action: impl FnOnce(&OwnedHandle) -> FixtureResult + Send,
	) -> io::Result<()> {
		self.on_thread(&self.reader, Role::Reader, action)
	}

	fn on_thread(
		&self,
		token: &OwnedHandle,
		role: Role,
		action: impl FnOnce(&OwnedHandle) -> FixtureResult + Send,
	) -> io::Result<()> {
		thread::scope(|scope| {
			let worker = scope.spawn(move || {
				let guard = Impersonation::begin(token)?;
				let effective = open_thread_token()?
					.ok_or_else(|| io::Error::other("missing effective thread token"))?;
				self.verify_effective(&effective, role)?;
				let result =
					action(&effective).map_err(|error| io::Error::other(error.to_string()));
				drop(guard);
				if open_thread_token()?.is_some() {
					return Err(io::Error::other(
						"the NTFS fixture thread retained impersonation",
					));
				}
				result
			});
			worker
				.join()
				.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
		})
	}

	fn verify_effective(&self, token: &OwnedHandle, role: Role) -> io::Result<()> {
		assert!(
			token_sid(token, USER_CLASS)? == self.user,
			"fixture must remain the same user"
		);
		assert!(
			!membership(token.as_raw_handle(), &self.administrators)?,
			"fixture token must lack enabled Builtin Administrators membership"
		);
		let traverse_enabled = check_privileges(token)?;
		match role {
			Role::Creator => {
				assert!(
					!is_restricted(token),
					"creator must use the user ACE without an additional SID restriction"
				);
				eprintln!(
					"Same-user NTFS creator: source admin enabled={}, effective admin enabled=false, enabled traverse privilege={traverse_enabled}; all other privileges disabled",
					self.source_administrator
				);
			}
			Role::Reader => {
				assert!(is_restricted(token), "reader must carry a restricting SID");
				let groups =
					TokenInformation::query(token, TokenRestrictedSids, size_of::<TOKEN_GROUPS>())?;
				// SAFETY: the query returned one or more aligned SDK group slots and the fixed header.
				let groups = unsafe { &*groups.as_ptr().cast::<TOKEN_GROUPS>() };
				assert_eq!(groups.GroupCount, 1);
				// SAFETY: GetTokenInformation returned the live SID pointer owned by the buffer.
				assert_ne!(
					unsafe { EqualSid(groups.Groups[0].Sid, self.world.as_sid()) },
					0
				);
			}
		}
		Ok(())
	}
}

pub(in crate::windows) fn with_reduced_token(
	action: impl FnOnce() -> FixtureResult + Send,
) -> io::Result<()> {
	Actors::for_current_user()?.as_creator(|_| action())
}

pub(super) fn source_user() -> io::Result<String> {
	token_sid(&process_token(TOKEN_QUERY)?, USER_CLASS)
}

fn process_token(access: u32) -> io::Result<OwnedHandle> {
	if open_thread_token()?.is_some() {
		return Err(io::Error::other(
			"the NTFS fixture requires a process-token baseline",
		));
	}
	let mut token = ptr::null_mut();
	// SAFETY: the current-process pseudohandle is valid and the output slot is writable.
	win32(unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut token) })?;
	// SAFETY: successful OpenProcessToken transfers the newly opened handle.
	Ok(unsafe { OwnedHandle::from_raw_handle(token) })
}

fn restrict(
	source: &OwnedHandle,
	administrators: &WellKnownSid,
	world: Option<&WellKnownSid>,
) -> io::Result<OwnedHandle> {
	let disabled = SID_AND_ATTRIBUTES {
		Sid: administrators.as_sid(),
		Attributes: 0,
	};
	let restricting = world.map(|sid| SID_AND_ATTRIBUTES {
		Sid: sid.as_sid(),
		Attributes: 0,
	});
	let mut token = ptr::null_mut();
	// SAFETY: source carries TOKEN_DUPLICATE. Each nonnull input array is one live SID slot
	// and the output slot holds one handle. The restricted token inherits the source handle's rights.
	win32(unsafe {
		CreateRestrictedToken(
			source.as_raw_handle(),
			DISABLE_MAX_PRIVILEGE,
			1,
			&disabled,
			0,
			ptr::null(),
			u32::from(restricting.is_some()),
			restricting.as_ref().map_or(ptr::null(), std::ptr::from_ref),
			&mut token,
		)
	})?;
	// SAFETY: successful CreateRestrictedToken transfers a valid owned primary token.
	Ok(unsafe { OwnedHandle::from_raw_handle(token) })
}

struct Impersonation(PhantomData<Rc<()>>);

impl Impersonation {
	fn begin(token: &OwnedHandle) -> io::Result<Self> {
		if open_thread_token()?.is_some() {
			return Err(io::Error::other(
				"nested NTFS fixture impersonation is unsupported",
			));
		}
		// SAFETY: this live same-user primary token has TOKEN_QUERY and TOKEN_DUPLICATE.
		// Success changes only this thread's effective Windows token.
		win32(unsafe { ImpersonateLoggedOnUser(token.as_raw_handle()) })?;
		Ok(Self(PhantomData))
	}
}

impl Drop for Impersonation {
	fn drop(&mut self) {
		// SAFETY: the non-Send guard is dropped on the same thread that began impersonation.
		if unsafe { RevertToSelf() } == 0 {
			eprintln!(
				"Windows NTFS fixture could not restore thread security: {}",
				io::Error::last_os_error()
			);
			// Windows requires process shutdown if restoration fails. This is the opt-in test process.
			std::process::exit(1);
		}
	}
}

fn open_thread_token() -> io::Result<Option<OwnedHandle>> {
	let mut token = ptr::null_mut();
	// SAFETY: current-thread pseudohandle is valid and the token output slot is writable.
	if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } != 0 {
		// SAFETY: successful OpenThreadToken transfers the newly opened handle.
		return Ok(Some(unsafe { OwnedHandle::from_raw_handle(token) }));
	}
	let error = io::Error::last_os_error();
	if error.raw_os_error() == Some(ERROR_NO_TOKEN.cast_signed()) {
		return Ok(None);
	}
	Err(error)
}

fn membership(token: HANDLE, sid: &WellKnownSid) -> io::Result<bool> {
	let mut enabled = 0;
	// SAFETY: token is a live impersonation token, or null for the caller's effective token.
	// The SID is initialized and the membership output slot is writable.
	win32(unsafe { CheckTokenMembership(token, sid.as_sid(), &mut enabled) })?;
	Ok(enabled != 0)
}

fn is_restricted(token: &OwnedHandle) -> bool {
	// SAFETY: token is a live query handle returned by the OS.
	unsafe { IsTokenRestricted(token.as_raw_handle()) != 0 }
}

fn check_privileges(token: &OwnedHandle) -> io::Result<bool> {
	let offset = offset_of!(TOKEN_PRIVILEGES, Privileges);
	let data = TokenInformation::query(token, TokenPrivileges, offset)?;
	// SAFETY: the checked minimum includes the aligned SDK count field, including for an empty list.
	let count =
		unsafe { (&raw const (*data.as_ptr().cast::<TOKEN_PRIVILEGES>()).PrivilegeCount).read() }
			as usize;
	let length = count
		.checked_mul(size_of::<LUID_AND_ATTRIBUTES>())
		.and_then(|len| offset.checked_add(len));
	if !length.is_some_and(|length| length <= data.used) {
		return Err(io::Error::other("truncated NTFS fixture privilege list"));
	}
	// SAFETY: the naturally aligned SDK tail range is in the returned initialized buffer.
	let entries = unsafe {
		slice::from_raw_parts(
			data.as_ptr()
				.cast::<u8>()
				.add(offset)
				.cast::<LUID_AND_ATTRIBUTES>(),
			count,
		)
	};
	let mut traverse = LUID::default();
	// SAFETY: the SDK privilege name is NUL-terminated and the LUID output is writable.
	win32(unsafe { LookupPrivilegeValueW(ptr::null(), SE_CHANGE_NOTIFY_NAME, &mut traverse) })?;
	let mut traverse_enabled = false;
	for entry in entries
		.iter()
		.filter(|entry| entry.Attributes & SE_PRIVILEGE_ENABLED != 0)
	{
		assert!(
			entry.Luid.LowPart == traverse.LowPart && entry.Luid.HighPart == traverse.HighPart,
			"fixture may retain only SeChangeNotifyPrivilege enabled"
		);
		traverse_enabled = true;
	}
	Ok(traverse_enabled)
}

struct WellKnownSid([u32; SID_WORDS]);

impl WellKnownSid {
	fn new(kind: WELL_KNOWN_SID_TYPE) -> io::Result<Self> {
		let mut sid = Self([0; SID_WORDS]);
		let mut size = SECURITY_MAX_SID_SIZE;
		// SAFETY: the output is naturally aligned and covers the recorded SDK SID maximum.
		win32(unsafe {
			CreateWellKnownSid(kind, ptr::null_mut(), sid.0.as_mut_ptr().cast(), &mut size)
		})?;
		Ok(sid)
	}

	fn as_sid(&self) -> PSID {
		self.0.as_ptr().cast_mut().cast()
	}
}

pub(super) struct TokenInformation {
	words: Vec<usize>,
	used: usize,
}

impl TokenInformation {
	pub(super) fn query(token: &OwnedHandle, class: i32, minimum: usize) -> io::Result<Self> {
		let mut required = 0;
		// SAFETY: a null zero-length output is the documented size probe for these classes.
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
		if result != 0 {
			return Err(io::Error::other(
				"unexpected empty NTFS fixture token query",
			));
		}
		if error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER.cast_signed()) {
			return Err(error);
		}
		if !(minimum..=TOKEN_INFORMATION_LIMIT).contains(&(required as usize)) {
			return Err(io::Error::other("invalid NTFS fixture token buffer length"));
		}
		let mut words = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
		let mut returned = 0;
		// SAFETY: initialized pointer-aligned buffer covers required bytes and remains owned.
		win32(unsafe {
			GetTokenInformation(
				token.as_raw_handle(),
				class,
				words.as_mut_ptr().cast(),
				required,
				&mut returned,
			)
		})?;
		let used = returned as usize;
		if !(minimum..=required as usize).contains(&used) {
			return Err(io::Error::other("invalid NTFS fixture token result length"));
		}
		Ok(Self { words, used })
	}

	pub(super) fn as_ptr(&self) -> *const core::ffi::c_void {
		self.words.as_ptr().cast()
	}
}
