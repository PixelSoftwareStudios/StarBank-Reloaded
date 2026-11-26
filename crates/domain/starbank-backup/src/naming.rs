use std::error::Error;
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

const BANK_EXTENSION: &str = "sc2bank";
const BACKUP_EXTENSION: &str = "sc2bank.bak";
const MAX_COMPONENT_BYTES: usize = 255;
const UTC_STAMP_BYTES: usize = 15;
const DATE_BYTES: usize = 8;
pub(crate) const COLLISION_ATTEMPTS: u16 = 1_000;

/// A UTC timestamp rendered as `YYYYMMDD_HHMMSS`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackupTimestamp([u8; UTC_STAMP_BYTES]);

impl FromStr for BackupTimestamp {
	type Err = NamingError;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		let bytes: [u8; UTC_STAMP_BYTES] = value
			.as_bytes()
			.try_into()
			.map_err(|_| NamingError::Timestamp)?;
		if bytes[DATE_BYTES] != b'_'
			|| !bytes[..DATE_BYTES].iter().all(u8::is_ascii_digit)
			|| !bytes[DATE_BYTES + 1..].iter().all(u8::is_ascii_digit)
		{
			return Err(NamingError::Timestamp);
		}
		let year = value[..4]
			.parse::<u16>()
			.map_err(|_| NamingError::Timestamp)?;
		let month = decimal_pair(&bytes[4..6]);
		let day = decimal_pair(&bytes[6..DATE_BYTES]);
		let hour = decimal_pair(&bytes[DATE_BYTES + 1..11]);
		let minute = decimal_pair(&bytes[11..13]);
		let second = decimal_pair(&bytes[13..]);
		let leap_year =
			year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
		let month_days = match month {
			1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
			4 | 6 | 9 | 11 => 30,
			2 if leap_year => 29,
			2 => 28,
			_ => return Err(NamingError::Timestamp),
		};
		if year == 0 || day == 0 || day > month_days || hour > 23 || minute > 59 || second > 59 {
			return Err(NamingError::Timestamp);
		}
		Ok(Self(bytes))
	}
}

impl TryFrom<&str> for BackupTimestamp {
	type Error = NamingError;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		value.parse()
	}
}

impl Display for BackupTimestamp {
	fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
		formatter.write_str(std::str::from_utf8(&self.0).map_err(|_| fmt::Error)?)
	}
}

fn decimal_pair(digits: &[u8]) -> u8 {
	(digits[0] - b'0') * 10 + (digits[1] - b'0')
}

/// The exact locally authorized bank path, with a portable backup basename.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BankSource {
	path: PathBuf,
	basename: String,
}

impl BankSource {
	/// Validates path shape and the bank's final filename. Authorization belongs to storage.
	///
	/// # Errors
	///
	/// Rejects non-absolute or parent-relative paths, and names that cannot be preserved
	/// as portable backup filename components.
	pub fn new(path: PathBuf) -> Result<Self, NamingError> {
		validate_location(&path)?;
		if !path
			.extension()
			.is_some_and(|extension| extension.eq_ignore_ascii_case(OsStr::new(BANK_EXTENSION)))
		{
			return Err(NamingError::BankName);
		}
		let basename = path
			.file_stem()
			.and_then(OsStr::to_str)
			.filter(|name| {
				!name.is_empty()
					&& !name.chars().any(|character| {
						character.is_control()
							|| matches!(
								character,
								'<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
							)
					})
			})
			.ok_or(NamingError::BankName)?
			.to_owned();
		Ok(Self { path, basename })
	}

	/// Returns the authorized path passed to the constructor.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}
}

/// A catalog key that also resolves to the individual snapshot's path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackupId(PathBuf);

impl BackupId {
	pub(crate) fn for_source(
		source: &BankSource,
		directory: &Path,
		timestamp: BackupTimestamp,
		sequence: u16,
	) -> Result<Self, NamingError> {
		validate_location(directory)?;
		if sequence >= COLLISION_ATTEMPTS {
			return Err(NamingError::Sequence);
		}
		let basename = &source.basename;
		let name = if sequence == 0 {
			format!("{basename}_{timestamp}.{BACKUP_EXTENSION}")
		} else {
			format!("{basename}_{timestamp}_{sequence}.{BACKUP_EXTENSION}")
		};
		if name.len() > MAX_COMPONENT_BYTES {
			return Err(NamingError::NameTooLong);
		}
		Ok(Self(directory.join(name)))
	}

	/// Returns the snapshot path for local storage use.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.0
	}
}

/// Where an authorized backup snapshot will be created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupPlacement<'a> {
	/// Alongside the live bank.
	Adjacent,
	/// In a caller-selected, authorized managed directory.
	Managed(&'a Path),
}

/// Timestamp and placement supplied by the caller for a snapshot operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackupRequest<'a> {
	/// Timestamp in UTC.
	pub timestamp: BackupTimestamp,
	/// Target placement.
	pub placement: BackupPlacement<'a>,
}

impl<'a> BackupRequest<'a> {
	/// Selects the source bank's containing directory.
	#[must_use]
	pub fn adjacent(timestamp: BackupTimestamp) -> Self {
		Self {
			timestamp,
			placement: BackupPlacement::Adjacent,
		}
	}

	pub(crate) fn directory(self, source: &'a BankSource) -> Result<&'a Path, NamingError> {
		match self.placement {
			BackupPlacement::Adjacent => source.path().parent().ok_or(NamingError::Location),
			BackupPlacement::Managed(directory) => {
				validate_location(directory)?;
				Ok(directory)
			}
		}
	}
}

fn validate_location(path: &Path) -> Result<(), NamingError> {
	if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
		return Err(NamingError::Location);
	}
	Ok(())
}

/// Invalid data for a portable timestamped backup path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NamingError {
	/// The path must be absolute and have no parent traversal.
	Location,
	/// The source filename has no supported bank extension or portable basename.
	BankName,
	/// The UTC timestamp is incorrectly formatted or not a real date.
	Timestamp,
	/// The collision sequence exceeds the supported suffix range.
	Sequence,
	/// The backup filename component exceeds the portable byte limit.
	NameTooLong,
}

impl Display for NamingError {
	fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Location => "backup locations require absolute paths without parent traversal",
			Self::BankName => "bank path requires a portable .sc2bank filename",
			Self::Timestamp => "backup timestamp requires a UTC date in YYYYMMDD_HHMMSS format",
			Self::Sequence => "backup collision sequence exceeds the supported range",
			Self::NameTooLong => "backup filename exceeds the portable component limit",
		})
	}
}

impl Error for NamingError {}
