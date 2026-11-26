mod commit;
#[cfg(target_os = "macos")]
mod darwin;
mod destination;
mod stage;

pub use commit::commit;
pub use destination::read_snapshot;
pub(crate) use destination::{FileIdentity, ObservedFile};
