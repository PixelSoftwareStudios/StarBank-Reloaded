use std::fmt;

use starbank_core::is_sc2_account_handle;

/// A field required to construct a signing context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextField {
	/// The handle of the map author.
	Author,
	/// The handle of the bank owner.
	Player,
	/// The intended bank filename stem.
	BankName,
}

impl fmt::Display for ContextField {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Author => "author handle",
			Self::Player => "player handle",
			Self::BankName => "bank name",
		})
	}
}

/// A required signing input was missing or did not have its expected form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidContext {
	/// A required input was empty.
	Empty(ContextField),
	/// A handle did not match the StarCraft II handle structure.
	Handle(ContextField),
	/// The input was a reserved path component or contained a path separator or NUL.
	BankName,
}

impl fmt::Display for InvalidContext {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Empty(field) => write!(formatter, "missing {field}"),
			Self::Handle(field) => write!(formatter, "invalid {field}"),
			Self::BankName => formatter.write_str("invalid bank name"),
		}
	}
}

impl std::error::Error for InvalidContext {}

/// Validated caller-supplied signing text, in the exact spelling to hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningContext {
	author: String,
	player: String,
	bank_name: String,
}

impl SigningContext {
	/// Checks the two handles and the intended bank filename stem.
	///
	/// The caller supplies the stem after removing the file extension. Leading zeroes,
	/// case, and whitespace in the stem are preserved. The caller determines whether
	/// the context belongs to the bank being verified or exported.
	///
	/// # Errors
	///
	/// Returns an error for empty fields, handles outside the SC2 handle structure,
	/// or a bank name containing a path separator or NUL or equal to `.` or `..`.
	pub fn new(
		author: impl Into<String>,
		player: impl Into<String>,
		bank_name: impl Into<String>,
	) -> Result<Self, InvalidContext> {
		let author = author.into();
		let player = player.into();
		let bank_name = bank_name.into();
		validate_handle(&author, ContextField::Author)?;
		validate_handle(&player, ContextField::Player)?;
		if bank_name.is_empty() {
			return Err(InvalidContext::Empty(ContextField::BankName));
		}
		if matches!(bank_name.as_str(), "." | "..") || bank_name.contains(['/', '\\', '\0']) {
			return Err(InvalidContext::BankName);
		}
		Ok(Self {
			author,
			player,
			bank_name,
		})
	}

	/// Returns the unmodified author handle.
	pub fn author(&self) -> &str {
		&self.author
	}

	/// Returns the unmodified player handle.
	pub fn player(&self) -> &str {
		&self.player
	}

	/// Returns the unmodified filename stem.
	pub fn bank_name(&self) -> &str {
		&self.bank_name
	}
}

fn validate_handle(handle: &str, field: ContextField) -> Result<(), InvalidContext> {
	if handle.is_empty() {
		return Err(InvalidContext::Empty(field));
	}
	if !is_sc2_account_handle(handle) {
		return Err(InvalidContext::Handle(field));
	}
	Ok(())
}
