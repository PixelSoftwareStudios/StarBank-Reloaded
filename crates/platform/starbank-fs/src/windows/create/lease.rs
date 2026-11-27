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

const LEASE_PREFIX: &str = "Global\\starbank-create-v1-";

pub(super) struct Lease {
	handle: OwnedHandle,
	// Win32 mutex ownership belongs to the acquiring thread, not merely the process.
	_thread: PhantomData<Rc<()>>,
}

impl Lease {
	pub(super) fn acquire(identity: Identity) -> Result<Self, StorageError> {
		let key = format!(
			"{LEASE_PREFIX}{:016x}-{}",
			identity.volume,
			hex::encode(identity.file_id)
		);
		let name: Vec<u16> = key.encode_utf16().chain([0]).collect();
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
				Err(busy(
					"a previous Windows writer stopped while holding the directory lease",
				))
			}
			WAIT_TIMEOUT => Err(busy("another Windows writer holds the directory lease")),
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

fn busy(message: &'static str) -> StorageError {
	StorageError::io(
		"acquire Windows directory lease",
		io::Error::new(io::ErrorKind::ResourceBusy, message),
	)
}
