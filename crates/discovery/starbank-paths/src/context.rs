use std::ffi::OsStr;
use std::path::{Component, Path};

pub(crate) const GAME_DIRECTORY: &str = "StarCraft II";
pub(crate) const ACCOUNTS: &str = "Accounts";
pub(crate) const BANKS: &str = "Banks";
pub(crate) const BANK_EXTENSION: &str = "sc2bank";
pub(crate) const BACKUP_EXTENSION: &str = "bak";

/// Exact lexical signing inputs inferred from a canonical bank layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SigningContextHint {
	/// Player handle from the account-scoped directory.
	pub player: String,
	/// Author handle from the first directory under Banks.
	pub author: String,
	/// Bank filename without its final extension.
	pub bank_name: String,
}

/// Infers context from an absolute Accounts/account/player/Banks/author bank path.
///
/// This inspects path components only. The caller owns path authorization and must
/// confirm a context before using it to sign. Direct, unknown and ambiguous layouts
/// return `None`.
pub fn parse_signing_context(path: &Path) -> Option<SigningContextHint> {
	if !path.is_absolute() || !has_extension(path, BANK_EXTENSION) {
		return None;
	}
	let normal = path
		.parent()?
		.components()
		.filter_map(|part| {
			if let Component::Normal(name) = part {
				Some(name)
			} else {
				None
			}
		})
		.collect::<Vec<_>>();
	if path.components().any(|part| part == Component::ParentDir) {
		return None;
	}
	let mut candidate = None;
	for window in normal.windows(5) {
		let [accounts, account, player, banks, author] = window else {
			continue;
		};
		if !accounts.eq_ignore_ascii_case(ACCOUNTS)
			|| !banks.eq_ignore_ascii_case(BANKS)
			|| !valid_numeric_account(account)
		{
			continue;
		}
		let Some(player) = valid_handle(player) else {
			continue;
		};
		let Some(author) = valid_handle(author) else {
			continue;
		};
		if candidate.replace((player, author)).is_some() {
			return None;
		}
	}
	let (player, author) = candidate?;
	Some(SigningContextHint {
		player: player.to_owned(),
		author: author.to_owned(),
		bank_name: path.file_stem()?.to_str()?.to_owned(),
	})
}

pub(crate) fn valid_numeric_account(name: &OsStr) -> bool {
	!name.is_empty() && name.as_encoded_bytes().iter().all(u8::is_ascii_digit)
}

pub(crate) fn valid_handle(name: &OsStr) -> Option<&str> {
	let name = name.to_str()?;
	let mut parts = name.split('-');
	let [Some(region), Some(game), Some(realm), Some(id), None] = [
		parts.next(),
		parts.next(),
		parts.next(),
		parts.next(),
		parts.next(),
	] else {
		return None;
	};
	if [region, realm, id]
		.into_iter()
		.all(|part| !part.is_empty() && part.bytes().all(|item| item.is_ascii_digit()))
		&& game == "S2"
	{
		Some(name)
	} else {
		None
	}
}

pub(crate) fn has_extension(path: &Path, extension: &str) -> bool {
	path.extension()
		.is_some_and(|value| value.eq_ignore_ascii_case(extension))
}
