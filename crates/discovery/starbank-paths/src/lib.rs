//! Native StarCraft II bank discovery over user-selected or auto-detected roots.
#![cfg_attr(not(windows), forbid(unsafe_code))]
#![cfg_attr(windows, deny(unsafe_code))]

mod context;
mod error;
mod inventory;
mod resolver;
mod selection;
#[cfg(windows)]
mod windows;

pub use context::{SigningContextHint, parse_signing_context};
pub use error::{CandidateFailure, LimitKind, PathError};
pub use inventory::{BankEntry, BankInventory, BankLocation, ScanLimits, inventory_banks};
pub use resolver::{AutoDetectedRoot, get_sc2_root_path};
pub use selection::{BankRoot, PathSelection, PlayerScope, SelectionKind, is_valid_bank_path};
