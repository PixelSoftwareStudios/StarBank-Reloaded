use std::{collections::BTreeMap, path::PathBuf};

use starbank_paths::{BankInventory, BankLocation};

use crate::{MapMetadata, metadata::normalized_bank_name};

/// One cache archive and its extracted metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapRecord {
	/// Native absolute archive path. This is the identity of the observed copy.
	pub archive: PathBuf,
	/// Cache file modification time as nanoseconds from the Unix epoch.
	pub modified_nanos: Option<i128>,
	/// Fields decoded from the archive's metadata.
	pub metadata: MapMetadata,
}

/// Maps observed in the cache, with a bank-name lookup.
#[derive(Debug, Default)]
pub struct MapIndex {
	maps: Vec<MapRecord>,
	by_bank: BTreeMap<String, Vec<usize>>,
}

impl MapIndex {
	pub(crate) fn from_records(mut maps: Vec<MapRecord>) -> Self {
		// A newer archive is shown first among plausible matches. Neither the
		// author display name nor title is a publication ID, so retain older
		// candidates and collisions instead of guessing which archive to discard.
		maps.sort_unstable_by(|left, right| {
			right
				.modified_nanos
				.cmp(&left.modified_nanos)
				.then_with(|| left.archive.cmp(&right.archive))
		});
		let mut by_bank = BTreeMap::<String, Vec<usize>>::new();
		for (position, entry) in maps.iter().enumerate() {
			for bank in &entry.metadata.bank_names {
				by_bank.entry(bank.clone()).or_default().push(position);
			}
		}
		Self { maps, by_bank }
	}

	/// Returns all cached map copies, newest first, with deterministic ties.
	pub fn maps(&self) -> &[MapRecord] {
		&self.maps
	}

	/// Associates this cache with the current bank inventory.
	///
	/// Associations indicate declared names only. A name match retains each
	/// location's original account/author scope and every candidate archive.
	pub fn associate<'a>(&'a self, inventory: &'a BankInventory) -> MapAssociations<'a> {
		let mut banks_by_name = BTreeMap::<String, Vec<&BankLocation>>::new();
		for bank in &inventory.banks {
			for location in &bank.locations {
				if let Some(name) = bank_name(location) {
					banks_by_name.entry(name).or_default().push(location);
				}
			}
		}
		MapAssociations {
			index: self,
			inventory,
			banks_by_name,
		}
	}
}

/// Local, read-only view of potential map and bank links.
#[derive(Debug)]
pub struct MapAssociations<'a> {
	index: &'a MapIndex,
	inventory: &'a BankInventory,
	banks_by_name: BTreeMap<String, Vec<&'a BankLocation>>,
}

impl<'a> MapAssociations<'a> {
	/// Returns all archive positions whose scripts claim this bank's basename.
	///
	/// Positions index `MapIndex::maps()`; several positions indicate ambiguity.
	pub fn candidates(&self, bank: &BankLocation) -> &[usize] {
		bank_name(bank)
			.and_then(|name| self.index.by_bank.get(&name))
			.map_or(&[], Vec::as_slice)
	}

	/// Returns all bank locations with names declared by this archive.
	pub fn banks_for_map(&self, map: &'a MapRecord) -> impl Iterator<Item = &'a BankLocation> + '_ {
		map.metadata
			.bank_names
			.iter()
			.filter_map(|name| self.banks_by_name.get(name))
			.flatten()
			.copied()
	}

	/// Returns banks for which the cache has no map claim.
	pub fn unknown_banks(&self) -> impl Iterator<Item = &'a BankLocation> + '_ {
		self.inventory
			.banks
			.iter()
			.flat_map(|bank| &bank.locations)
			.filter(|location| self.candidates(location).is_empty())
	}

	/// Returns cached archive records for which no bank is present.
	pub fn maps_without_banks(&self) -> impl Iterator<Item = &'a MapRecord> + '_ {
		self.index
			.maps
			.iter()
			.filter(|map| self.banks_for_map(map).next().is_none())
	}
}

fn bank_name(location: &BankLocation) -> Option<String> {
	normalized_bank_name(location.path.file_stem()?.to_str()?)
}
