use std::{
	borrow::Cow,
	ffi::OsStr,
	path::{Path, PathBuf},
};

use serde::Serialize;
use starbank_paths::{
	BankLocation, ScanLimits, SelectionKind, get_sc2_root_path, inventory_banks, is_valid_bank_path,
};

use crate::failure::Failure;

pub(crate) fn discover(root: Option<&Path>) -> Result<Discovery, Failure> {
	let limits = ScanLimits::default();
	let (selection, candidate_error_count) = match root {
		Some(path) => (is_valid_bank_path(path, limits)?, 0),
		None => {
			let detected = get_sc2_root_path(limits)?;
			(detected.selection, detected.diagnostics.len())
		}
	};
	let inventory = inventory_banks(&selection, limits)?;
	Ok(Discovery {
		root: selection.path().to_owned(),
		kind: selection.kind().into(),
		candidate_error_count,
		banks: inventory
			.banks
			.into_iter()
			.map(|entry| entry.locations)
			.collect(),
		backups: inventory.backups,
	})
}

pub(crate) struct Discovery {
	root: PathBuf,
	kind: Kind,
	candidate_error_count: usize,
	banks: Vec<Vec<BankLocation>>,
	backups: Vec<PathBuf>,
}

impl Discovery {
	pub(crate) fn report(&self) -> DiscoveryReport<'_> {
		DiscoveryReport {
			root: DisplayPath::new(&self.root),
			kind: self.kind,
			candidate_error_count: self.candidate_error_count,
			banks: self
				.banks
				.iter()
				.map(|locations| {
					locations
						.iter()
						.map(|location| BankLocationReport {
							path: DisplayPath::new(&location.path),
							account: location.scope.account.as_deref().and_then(OsStr::to_str),
							player: location.scope.player.as_deref(),
							signing_context_available: location.signing_hint.is_some(),
						})
						.collect()
				})
				.collect(),
			backups: self
				.backups
				.iter()
				.map(|path| DisplayPath::new(path))
				.collect(),
		}
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct DiscoveryReport<'path> {
	pub(crate) root: DisplayPath<'path>,
	kind: Kind,
	candidate_error_count: usize,
	pub(crate) banks: Vec<Vec<BankLocationReport<'path>>>,
	pub(crate) backups: Vec<DisplayPath<'path>>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
	GameRoot,
	Accounts,
	Banks,
	Collection,
	File,
}

impl From<SelectionKind> for Kind {
	fn from(kind: SelectionKind) -> Self {
		match kind {
			SelectionKind::GameRoot => Self::GameRoot,
			SelectionKind::Accounts => Self::Accounts,
			SelectionKind::Banks => Self::Banks,
			SelectionKind::Collection => Self::Collection,
			SelectionKind::File => Self::File,
		}
	}
}

#[derive(Debug, Serialize)]
pub(crate) struct BankLocationReport<'path> {
	path: DisplayPath<'path>,
	account: Option<&'path str>,
	player: Option<&'path str>,
	signing_context_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct DisplayPath<'path> {
	pub(crate) display: Cow<'path, str>,
	lossless: bool,
}

impl<'path> DisplayPath<'path> {
	pub(crate) fn new(path: &'path Path) -> Self {
		Self {
			display: path.to_string_lossy(),
			lossless: path.to_str().is_some(),
		}
	}
}
