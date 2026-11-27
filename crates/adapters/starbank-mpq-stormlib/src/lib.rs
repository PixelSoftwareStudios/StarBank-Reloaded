//! Optional native append backend for independently verified classic MPQ candidates.
#![deny(unsafe_code)]

#[cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]
mod append;
#[cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]
mod error;
#[cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]
mod native;
#[cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]
mod scratch;

#[cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]
pub use append::append;
#[cfg(all(
	feature = "native-stormlib",
	target_os = "macos",
	target_arch = "aarch64"
))]
pub use error::{AppendError, NativeOperation, ScratchOperation};
