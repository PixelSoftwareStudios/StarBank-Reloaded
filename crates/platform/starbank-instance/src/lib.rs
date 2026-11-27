//! Bounded launch messages and local running-instance delivery on Windows.
#![deny(unsafe_code)]

#[cfg(any(windows, test))]
mod frame;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
#[path = "windows/security.rs"]
mod windows_security;

#[cfg(any(windows, test))]
pub use frame::{FrameError, MAX_FRAME_LEN, MAX_PATHS, OpenRequest, QueueAdmission};
#[cfg(windows)]
pub use windows::{
	Claim, EndpointGuard, InstanceError, Listener, PipeOperation, claim, is_bank_candidate,
};
