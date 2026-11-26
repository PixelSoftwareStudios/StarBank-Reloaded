//! Matches declarative game profiles and proposes atomic edits to bound bank values.

#![forbid(unsafe_code)]

mod binding;
mod decimal;
mod definition;
mod edit;
mod legacy;
mod profile;

pub use binding::{
	BankIdentity, BoundField, BoundGroup, BoundProfile, Confidence, MatchOutcome, ProfileRegistry,
};
pub use definition::{CheckboxStorage, Control, NumberStorage, TextStorage};
pub use edit::{ProfileEditError, ProfileInput, ProposedValueEdit};
pub use profile::{Profile, ProfileError};
