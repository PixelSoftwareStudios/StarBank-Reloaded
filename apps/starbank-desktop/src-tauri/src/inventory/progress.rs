use std::time::{Duration, Instant};

use serde::Serialize;
use starbank_map_index::Progress;

const ARCHIVE_UPDATE_TARGET: usize = 32;
const MIN_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Serialize)]
pub(crate) struct InventoryProgress {
	generation: String,
	#[serde(flatten)]
	stage: InventoryStage,
}

#[derive(Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum InventoryStage {
	LocatingFolder,
	EnumeratingBanks,
	IndexingMaps,
	InspectingArchives { completed: usize, total: usize },
}

pub(super) struct InventoryProgressPublisher<F> {
	generation: u64,
	send: F,
	last_archives: Option<(usize, Instant)>,
}

impl<F: FnMut(InventoryProgress)> InventoryProgressPublisher<F> {
	pub(super) fn new(generation: u64, send: F) -> Self {
		Self {
			generation,
			send,
			last_archives: None,
		}
	}

	pub(super) fn locating_folder(&mut self) {
		self.publish(InventoryStage::LocatingFolder);
	}

	pub(super) fn enumerating_banks(&mut self) {
		self.publish(InventoryStage::EnumeratingBanks);
	}

	pub(super) fn indexing_maps(&mut self) {
		self.publish(InventoryStage::IndexingMaps);
	}

	pub(super) fn inspected_archive(&mut self, update: Progress<'_>) {
		let Progress {
			generation,
			completed,
			total,
			..
		} = update;
		if generation != self.generation || completed == 0 || completed > total {
			return;
		}
		if let Some((last_count, last_sent)) = self.last_archives {
			if completed <= last_count {
				return;
			}
			let step = total.div_ceil(ARCHIVE_UPDATE_TARGET);
			if completed != total
				&& (completed - last_count < step || last_sent.elapsed() < MIN_UPDATE_INTERVAL)
			{
				return;
			}
		}
		self.last_archives = Some((completed, Instant::now()));
		self.publish(InventoryStage::InspectingArchives { completed, total });
	}

	fn publish(&mut self, stage: InventoryStage) {
		(self.send)(InventoryProgress {
			generation: self.generation.to_string(),
			stage,
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn archive_progress_is_bounded_and_generation_safe() {
		let mut events = Vec::new();
		let generation = u64::MAX;
		let generation_label = generation.to_string();
		let mut publisher = InventoryProgressPublisher::new(generation, |event| {
			events.push(serde_json::to_value(event).expect("progress JSON"));
		});
		publisher.indexing_maps();
		let total = 2_048;
		publisher.inspected_archive(Progress {
			generation: generation - 1,
			completed: 1,
			total,
			record: None,
		});
		for completed in 1..=total {
			// Advance the clock without sleeping so only the count bound applies.
			if let Some((prior, _)) = publisher.last_archives {
				publisher.last_archives = Some((prior, Instant::now() - MIN_UPDATE_INTERVAL));
			}
			publisher.inspected_archive(Progress {
				generation,
				completed,
				total,
				record: None,
			});
		}
		publisher.inspected_archive(Progress {
			generation,
			completed: total,
			total,
			record: None,
		});
		assert!(events.len() <= ARCHIVE_UPDATE_TARGET + 2);
		assert_eq!(
			events[0],
			serde_json::json!({"generation": generation_label, "stage": "indexing_maps"})
		);
		assert_eq!(events[1]["completed"], 1);
		assert_eq!(events[1]["total"], total);
		assert_eq!(events.last().expect("final update")["completed"], total);
		assert!(
			events
				.iter()
				.all(|event| event["generation"] == generation_label)
		);
	}

	#[test]
	fn close_together_archive_events_are_coalesced_except_completion() {
		let mut events = Vec::new();
		let mut publisher = InventoryProgressPublisher::new(1, |event| {
			events.push(serde_json::to_value(event).expect("progress JSON"));
		});
		for completed in 1..=3 {
			if let Some((prior, _)) = publisher.last_archives {
				publisher.last_archives = Some((prior, Instant::now() + MIN_UPDATE_INTERVAL));
			}
			publisher.inspected_archive(Progress {
				generation: 1,
				completed,
				total: 3,
				record: None,
			});
		}
		assert_eq!(events.len(), 2);
		assert_eq!(events[0]["completed"], 1);
		assert_eq!(events[1]["completed"], 3);
	}
}
