use std::{
	collections::{BTreeMap, HashSet},
	ffi::OsStr,
	hash::{BuildHasher, RandomState},
	path::Path,
	sync::Arc,
};

use serde::Serialize;
use starbank_map_index::IndexReport;
use starbank_paths::BankInventory;
use starbank_paths::PlayerScope;

use super::{BankAuthority, authority::AuthorizedRoot, batch::MapMembers};

const MAX_BANK_TARGETS: usize = 4_096;
const MAX_MAP_MATCHES: usize = 49_152;
const MAX_DISPLAY_CHARS: usize = 512;
const BANK_PREFIX: &str = "bank-";
const MAP_PREFIX: &str = "map-";
const PLAYER_SCOPE_PREFIX: &str = "scope-";
const HEX_ID_CHARS: usize = 16;
const MAX_DATE_MILLIS: i64 = 8_640_000_000_000_000;
const NANOS_PER_MILLI: i128 = 1_000_000;

pub(super) type Projection = (InventorySnapshot, BTreeMap<String, BankAuthority>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InventoryStatus {
	Ready,
	RootNotFound,
	ManualRequired,
	Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InventoryReason {
	InvalidFolder,
	Unreadable,
	TooManyEntries,
	TooManyMatches,
	IdentifierConflict,
	ConfigurationUnavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InventorySnapshot {
	pub(super) status: InventoryStatus,
	pub(super) reason: Option<InventoryReason>,
	pub(super) maps: Vec<MapItem>,
	pub(super) banks: Vec<BankItem>,
	pub(super) root_name: Option<String>,
	pub(super) manual: bool,
	pub(super) incomplete: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MapItem {
	id: String,
	name: String,
	author: Option<String>,
	modified_at_millis: Option<i64>,
	is_blizzard: bool,
	has_bank_files: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BankItem {
	id: String,
	name: String,
	scope_id: Option<String>,
	player_handle: Option<String>,
	author_handle: Option<String>,
	map_ids: Vec<String>,
}

impl InventorySnapshot {
	pub(super) fn map_members(&self) -> MapMembers {
		let mut members: MapMembers = self
			.maps
			.iter()
			.map(|map| (map.id.clone(), Vec::new()))
			.collect();
		for bank in &self.banks {
			for id in &bank.map_ids {
				if let Some(map_banks) = members.get_mut(id) {
					map_banks.push(bank.id.clone());
				}
			}
		}
		members
	}

	pub(super) fn empty(status: InventoryStatus, reason: Option<InventoryReason>) -> Self {
		Self {
			status,
			reason,
			maps: Vec::new(),
			banks: Vec::new(),
			root_name: None,
			manual: false,
			incomplete: false,
		}
	}
}

/// Identities use a per-session opaque hash and never encode or expose a pathname.
/// The native table, not the hash itself, grants authority to open a bank.
#[derive(Clone, Default)]
pub(super) struct OpaqueIds(RandomState);

impl OpaqueIds {
	fn bank(&self, path: &Path) -> String {
		self.id(BANK_PREFIX, path)
	}

	fn map(&self, path: &Path) -> String {
		self.id(MAP_PREFIX, path)
	}

	fn player_scope(&self, scope: &PlayerScope) -> Option<String> {
		let identity = (scope.account.as_deref(), scope.player.as_deref());
		if identity == (None, None) {
			return None;
		}
		Some(format!(
			"{PLAYER_SCOPE_PREFIX}{:016x}",
			self.0.hash_one((PLAYER_SCOPE_PREFIX, identity))
		))
	}

	fn id(&self, prefix: &str, path: &Path) -> String {
		format!("{prefix}{:016x}", self.0.hash_one((prefix, path)))
	}

	pub(super) fn accepts_map_id(id: &str) -> bool {
		id.strip_prefix(MAP_PREFIX).is_some_and(|hex| {
			hex.len() == HEX_ID_CHARS && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
		})
	}

	pub(super) fn accepts_bank_id(id: &str) -> bool {
		id.strip_prefix(BANK_PREFIX).is_some_and(|hex| {
			hex.len() == HEX_ID_CHARS && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
		})
	}
}

pub(super) fn project(
	root: Arc<AuthorizedRoot>,
	manual: bool,
	inventory: &BankInventory,
	report: Option<&IndexReport>,
	ids: &OpaqueIds,
) -> Projection {
	let root_name = root.path().file_name().map(display_name);
	let mut snapshot = InventorySnapshot {
		root_name,
		manual,
		..InventorySnapshot::empty(InventoryStatus::Ready, None)
	};
	let mut targets = BTreeMap::new();
	let mut map_ids = HashSet::new();
	let mut total_matches = 0_usize;
	let associations = report.map(|report| report.index.associate(inventory));
	snapshot.incomplete =
		report.is_none_or(|report| report.incomplete || !report.issues.is_empty());
	if let Some(report) = report {
		for record in report.index.maps() {
			let id = ids.map(&record.archive);
			if !map_ids.insert(id.clone()) {
				return failed_projection(snapshot, InventoryReason::IdentifierConflict);
			}
			let name = record
				.metadata
				.title
				.clone()
				.or_else(|| record.archive.file_name().map(display_name))
				.unwrap_or_else(|| String::from("Unknown map"));
			snapshot.maps.push(MapItem {
				id,
				name,
				author: record.metadata.author.clone(),
				modified_at_millis: record.modified_nanos.and_then(epoch_millis),
				is_blizzard: record.metadata.declared_blizzard() == Some(true),
				has_bank_files: false,
			});
		}
	}
	for location in inventory.banks.iter().flat_map(|entry| &entry.locations) {
		let Some(target) = BankAuthority::new(&location.path, Arc::clone(&root)) else {
			snapshot.incomplete = true;
			continue;
		};
		let Some(name) = location.path.file_name() else {
			continue;
		};
		if targets.len() == MAX_BANK_TARGETS {
			return failed_projection(snapshot, InventoryReason::TooManyEntries);
		}
		let id = ids.bank(&location.path);
		if targets.insert(id.clone(), target).is_some() {
			return failed_projection(snapshot, InventoryReason::IdentifierConflict);
		}
		let matches = associations
			.as_ref()
			.map_or(&[][..], |association| association.candidates(location));
		total_matches = total_matches.saturating_add(matches.len());
		let mut map_ids = Vec::new();
		if total_matches <= MAX_MAP_MATCHES {
			for position in matches {
				if let Some(map) = snapshot.maps.get_mut(*position) {
					map.has_bank_files = true;
					map_ids.push(map.id.clone());
				}
			}
		}
		snapshot.banks.push(BankItem {
			id,
			name: display_name(name),
			scope_id: ids.player_scope(&location.scope),
			player_handle: location.scope.player.clone(),
			author_handle: location
				.signing_hint
				.as_ref()
				.map(|hint| hint.author.clone()),
			map_ids,
		});
	}
	if total_matches > MAX_MAP_MATCHES {
		// A partial set of links would turn an ambiguous association into a
		// misleading single result. Retain all bank authority for direct opens.
		snapshot.maps.clear();
		for bank in &mut snapshot.banks {
			bank.map_ids.clear();
		}
		snapshot.incomplete = true;
		snapshot.reason = Some(InventoryReason::TooManyMatches);
	}
	(snapshot, targets)
}

fn failed_projection(mut snapshot: InventorySnapshot, reason: InventoryReason) -> Projection {
	snapshot.status = InventoryStatus::Failed;
	snapshot.reason = Some(reason);
	snapshot.maps.clear();
	snapshot.banks.clear();
	(snapshot, BTreeMap::new())
}

pub(super) fn display_name(name: &OsStr) -> String {
	name.to_string_lossy()
		.chars()
		.filter(|value| !value.is_control())
		.take(MAX_DISPLAY_CHARS)
		.collect()
}

fn epoch_millis(nanos: i128) -> Option<i64> {
	let millis = i64::try_from(nanos.div_euclid(NANOS_PER_MILLI)).ok()?;
	(-MAX_DATE_MILLIS..=MAX_DATE_MILLIS)
		.contains(&millis)
		.then_some(millis)
}
