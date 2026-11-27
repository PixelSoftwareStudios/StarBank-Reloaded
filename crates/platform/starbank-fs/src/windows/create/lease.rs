//! Nonblocking directory-wide coordination between cooperating Windows writers.

use super::super::Identity;
use crate::StorageError;
use std::io;
use std::marker::PhantomData;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;
use std::rc::Rc;
use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
use windows_sys::Win32::{
	Foundation::{WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
	System::Threading::{CreateMutexExW, MUTEX_MODIFY_STATE, ReleaseMutex, WaitForSingleObject},
};

#[cfg(test)]
use windows_sys::Win32::System::Threading::OpenMutexW;

const LEASE_PREFIX: &str = "Global\\starbank-create-v1-";

pub(super) struct Lease {
	handle: OwnedHandle,
	// Win32 mutex ownership belongs to the acquiring thread, not merely the process.
	_thread: PhantomData<Rc<()>>,
}

impl Lease {
	pub(super) fn acquire(identity: Identity) -> Result<Self, StorageError> {
		let name = mutex_name(identity);
		// SAFETY: NUL-terminated stable name, no inherited handle, no initial ownership.
		// Global mutex creation does not require SeCreateGlobalPrivilege (unlike mappings
		// and symbolic links). The default token DACL is retained; denied access fails closed.
		let raw = unsafe {
			CreateMutexExW(
				ptr::null(),
				name.as_ptr(),
				0,
				MUTEX_MODIFY_STATE | SYNCHRONIZE,
			)
		};
		if raw.is_null() {
			return Err(StorageError::io(
				"open Windows directory lease",
				io::Error::last_os_error(),
			));
		}
		// SAFETY: successful creation/open transfers one owned mutex handle.
		let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
		// SAFETY: live mutex with synchronize access; zero timeout never blocks.
		match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
			WAIT_OBJECT_0 => Ok(Self {
				handle,
				_thread: PhantomData,
			}),
			WAIT_ABANDONED => {
				// Ownership was granted but a previous transaction may have stopped. Release
				// without inspecting, deleting or replaying any old artifact.
				let lease = Self {
					handle,
					_thread: PhantomData,
				};
				drop(lease);
				Err(busy(Contention::Abandoned))
			}
			WAIT_TIMEOUT => Err(busy(Contention::Busy)),
			WAIT_FAILED => Err(StorageError::io(
				"acquire Windows directory lease",
				io::Error::last_os_error(),
			)),
			_ => Err(StorageError::io(
				"acquire Windows directory lease",
				io::Error::other("unexpected mutex wait result"),
			)),
		}
	}
}

impl Drop for Lease {
	fn drop(&mut self) {
		// SAFETY: this thread acquired ownership; the !Send marker prevents transferring
		// the lease. Closing the owned handle follows the release attempt on every path.
		let _ = unsafe { ReleaseMutex(self.handle.as_raw_handle()) };
	}
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Contention {
	Busy,
	Abandoned,
}
impl std::fmt::Display for Contention {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::Busy => "another Windows writer holds the directory lease",
			Self::Abandoned => {
				"a previous Windows writer stopped while holding the directory lease"
			}
		})
	}
}
impl std::error::Error for Contention {}
fn busy(reason: Contention) -> StorageError {
	StorageError::io(
		"acquire Windows directory lease",
		io::Error::new(io::ErrorKind::ResourceBusy, reason),
	)
}
fn mutex_name(identity: Identity) -> Vec<u16> {
	format!(
		"{LEASE_PREFIX}{:016x}-{}",
		identity.volume,
		hex::encode(identity.file_id)
	)
	.encode_utf16()
	.chain([0])
	.collect()
}
#[cfg(test)]
pub(super) fn keep_existing_alive(identity: Identity) -> Result<OwnedHandle, StorageError> {
	let name = mutex_name(identity);
	// SAFETY: open only the existing mutex with synchronize access. This handle never
	// waits or owns it; keeping it live preserves abandoned state across process death.
	let raw = unsafe { OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) };
	if raw.is_null() {
		return Err(StorageError::io(
			"retain existing directory lease",
			io::Error::last_os_error(),
		));
	}
	// SAFETY: successful OpenMutexW transfers one owned handle.
	Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
