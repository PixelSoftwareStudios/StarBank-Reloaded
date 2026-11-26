use std::collections::BTreeMap;

use super::{
	InventoryError, InventoryService,
	authority::FrozenBankAuthority,
	projection::{OpaqueIds, display_name},
};

pub(super) type MapMembers = BTreeMap<String, Vec<String>>;

pub(crate) struct InventoryBatch {
	pub(crate) generation: u64,
	pub(crate) members: Vec<InventoryBatchMember>,
}

pub(crate) struct InventoryBatchMember {
	pub(crate) id: String,
	pub(crate) name: String,
	pub(crate) authority: FrozenBankAuthority,
}

impl InventoryService {
	pub(crate) fn capture_batch(
		&self,
		map_id: Option<&str>,
	) -> Result<InventoryBatch, InventoryError> {
		let (generation, targets) = {
			let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
			if state.active.is_some() {
				return Err(InventoryError::Busy);
			}
			let targets = if let Some(map_id) = map_id {
				if !OpaqueIds::accepts_map_id(map_id) {
					return Err(InventoryError::UnknownMap);
				}
				state
					.map_members
					.get(map_id)
					.ok_or(InventoryError::UnknownMap)?
					.iter()
					.map(|id| {
						state
							.banks
							.get(id)
							.map(|bank| (id.clone(), bank.clone()))
							.ok_or(InventoryError::StaleBank)
					})
					.collect::<Result<Vec<_>, _>>()?
			} else {
				state
					.banks
					.iter()
					.map(|(id, bank)| (id.clone(), bank.clone()))
					.collect()
			};
			(state.generation, targets)
		};
		let mut members = targets
			.into_iter()
			.map(|(id, bank)| {
				let name = bank
					.path()
					.file_name()
					.map(display_name)
					.unwrap_or_default();
				InventoryBatchMember {
					id,
					name,
					authority: bank.freeze(),
				}
			})
			.collect::<Vec<_>>();
		members.sort_unstable_by(|left, right| left.id.cmp(&right.id));
		members.dedup_by(|left, right| left.id == right.id);
		self.validate_batch_generation(generation)?;
		Ok(InventoryBatch {
			generation,
			members,
		})
	}

	pub(crate) fn validate_batch_generation(&self, generation: u64) -> Result<(), InventoryError> {
		let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
		if state.active.is_some() || state.generation != generation {
			return Err(InventoryError::StaleBank);
		}
		Ok(())
	}
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
pub(crate) fn scan_batch_fixture(root: &std::path::Path) -> Result<InventoryBatch, InventoryError> {
	let service = InventoryService::default();
	service.scan_with_roots(super::ScanMode::Manual(root.to_owned()), &[])?;
	service.capture_batch(None)
}
