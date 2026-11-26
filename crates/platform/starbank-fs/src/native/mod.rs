mod commit;
#[cfg(target_os = "macos")]
mod darwin;
mod destination;
mod stage;

pub(crate) use commit::commit_scoped;
pub use commit::{check_replace_scope, commit};
#[cfg(target_os = "macos")]
pub(crate) use darwin::require_local_apfs;
pub use destination::read_snapshot;
pub(crate) use destination::{FileIdentity, ObservedFile, open_directory, read_scoped};
