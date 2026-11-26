use serde::Serialize;
use starbank_fs::{BackupPresence, BackupPresenceDirectory, backup_source, scan_backup_presence};

use crate::backup_location::{ManagedBackupFolder, ManagedBackupHistory};

use super::{InventoryError, InventoryService};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventoryBackupStatus {
	bank_id: String,
	state: BackupPresence,
}

impl InventoryService {
	pub(super) fn recorded_backups(
		&self,
		generation: u64,
		history: impl FnOnce() -> ManagedBackupHistory,
	) -> Result<Vec<InventoryBackupStatus>, InventoryError> {
		let _lease = self
			.backup_status_scan
			.try_lock()
			.map_err(|_| InventoryError::Busy)?;
		let targets = {
			let state = self.state.lock().map_err(|_| InventoryError::Unavailable)?;
			if state.generation != generation || state.active.is_some() {
				return Err(InventoryError::StaleBank);
			}
			state.banks.clone()
		};
		let mut report = Vec::with_capacity(targets.len());
		let mut viable_positions = Vec::new();
		let mut sources = Vec::new();
		for (bank_id, authority) in targets {
			if authority.validate().is_ok()
				&& let Ok(source) = backup_source(authority.path())
			{
				sources.push(source);
				viable_positions.push(report.len());
			}
			report.push(InventoryBackupStatus {
				bank_id,
				state: BackupPresence::Unknown,
			});
		}
		if !sources.is_empty() {
			let history = history();
			let directories = history
				.folders
				.iter()
				.map(|folder| match folder {
					ManagedBackupFolder::Default(path) => BackupPresenceDirectory::Default(path),
					ManagedBackupFolder::Selected(grant) => {
						BackupPresenceDirectory::Selected(grant)
					}
				})
				.collect::<Vec<_>>();
			let states = scan_backup_presence(&sources, &directories);
			for (position, state) in viable_positions.into_iter().zip(states) {
				report[position].state = match state {
					BackupPresence::NeverBackedUp if history.unavailable > 0 => {
						BackupPresence::Unknown
					}
					_ => state,
				};
			}
		}
		self.validate_batch_generation(generation)?;
		Ok(report)
	}
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
	use std::{fs, io};

	use starbank_fs::BackupPlacement;

	use crate::backup::fresh_catalog;
	use crate::inventory::ScanMode;

	use super::*;

	#[test]
	fn status_is_generation_bound_redacted_and_never_negative_on_unavailable_history()
	-> io::Result<()> {
		let sandbox = tempfile::tempdir()?;
		let folder = sandbox.path().canonicalize()?;
		let bank = folder.join("HasBackup.SC2Bank");
		fs::write(&bank, b"<Bank/>")?;
		fs::write(folder.join("Empty.SC2Bank"), b"<Bank/>")?;
		fresh_catalog()
			.expect("catalog")
			.create(
				&backup_source(&bank).expect("bank source"),
				BackupPlacement::Adjacent,
			)
			.expect("adjacent backup");
		let inventory = InventoryService::default();
		inventory
			.scan_with_roots(ScanMode::Manual(folder.clone()), &[])
			.expect("synthetic inventory");
		let history = || ManagedBackupHistory {
			folders: Vec::new(),
			unavailable: 1,
		};
		let statuses = inventory
			.recorded_backups(1, history)
			.expect("current status");
		assert_eq!(statuses.len(), 2);
		assert!(
			statuses
				.iter()
				.any(|row| row.state == BackupPresence::Unknown)
		);
		assert!(
			statuses
				.iter()
				.any(|row| row.state == BackupPresence::Recorded)
		);
		let rendered = serde_json::to_string(&statuses)?;
		assert!(!rendered.contains(folder.to_string_lossy().as_ref()));
		assert!(matches!(
			inventory.recorded_backups(0, || panic!("stale request must never scan")),
			Err(InventoryError::StaleBank)
		));
		assert!(matches!(
			inventory.recorded_backups(1, || {
				inventory
					.scan_with_roots(ScanMode::Refresh, &[])
					.expect("new generation");
				history()
			}),
			Err(InventoryError::StaleBank)
		));
		Ok(())
	}
}
