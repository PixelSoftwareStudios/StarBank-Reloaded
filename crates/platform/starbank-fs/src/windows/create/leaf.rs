//! Restricts new names to the ordinary Win32 namespace used by the picker and reopen path.

use crate::StorageError;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;

const MAX_LEAF_UNITS: usize = 255;
const LAST_ASCII_CONTROL: u16 = 0x1f;
const RESERVED_PUNCTUATION: &[u8] = b"<>\"|?*";
const DEVICE_NAMES: [&[u8]; 6] = [b"CON", b"PRN", b"AUX", b"NUL", b"CONIN$", b"CONOUT$"];
const DEVICE_PREFIXES: [&[u8]; 2] = [b"COM", b"LPT"];
const SUPERSCRIPT_DEVICE_DIGITS: [u16; 3] = ['¹' as u16, '²' as u16, '³' as u16];

pub(super) fn validate(name: &OsStr) -> Result<(), StorageError> {
	super::super::check_component(name)?;
	let units: Vec<_> = name.encode_wide().take(MAX_LEAF_UNITS + 1).collect();
	if units.is_empty() || units.len() > MAX_LEAF_UNITS {
		return Err(StorageError::UnsafePath(
			"new Windows leaf length is unsupported",
		));
	}
	// Microsoft naming conventions exclude control characters and DOS device aliases,
	// including aliases before extensions and the three superscript COM/LPT digits.
	// https://learn.microsoft.com/windows/win32/fileio/naming-a-file
	if units.iter().copied().any(|unit| {
		unit <= LAST_ASCII_CONTROL
			|| u8::try_from(unit).is_ok_and(|byte| RESERVED_PUNCTUATION.contains(&byte))
	}) {
		return Err(StorageError::UnsafePath(
			"new Windows leaf uses a reserved character",
		));
	}
	let mut base = units
		.split(|&unit| unit == u16::from(b'.'))
		.next()
		.unwrap_or_default();
	while let Some(trimmed) = base.strip_suffix(&[u16::from(b' ')]) {
		base = trimmed;
	}
	if is_reserved_device(base) {
		return Err(StorageError::UnsafePath(
			"new Windows leaf is a reserved DOS device name",
		));
	}
	Ok(())
}

fn is_reserved_device(base: &[u16]) -> bool {
	DEVICE_NAMES
		.iter()
		.any(|device| eq_ignore_ascii_case(base, device))
		|| base.split_last().is_some_and(|(digit, prefix)| {
			((u16::from(b'1')..=u16::from(b'9')).contains(digit)
				|| SUPERSCRIPT_DEVICE_DIGITS.contains(digit))
				&& DEVICE_PREFIXES
					.iter()
					.any(|device| eq_ignore_ascii_case(prefix, device))
		})
}

fn eq_ignore_ascii_case(units: &[u16], ascii: &[u8]) -> bool {
	units.len() == ascii.len()
		&& units.iter().zip(ascii).all(|(&unit, byte)| {
			u8::try_from(unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(byte))
		})
}

#[cfg(test)]
mod tests {
	use super::{MAX_LEAF_UNITS, validate};
	use std::{
		ffi::{OsStr, OsString},
		os::windows::ffi::{OsStrExt as _, OsStringExt as _},
	};

	#[test]
	fn refuses_reserved_devices_extensions_and_superscript_digits() {
		for name in [
			"NUL",
			"nul.SC2Bank",
			"CON.tar.gz",
			"COM1.SC2Bank",
			"lpt9",
			"COM¹.SC2Bank",
			"LPT².txt",
			"COM³",
			"CONIN$",
			"CONOUT$",
			"AUX .txt",
			"a?b.SC2Bank",
			"a\u{1f}b",
		] {
			assert!(validate(OsStr::new(name)).is_err(), "reserved {name:?}");
		}
		for name in [
			"COM0.SC2Bank",
			"COM10.SC2Bank",
			"LPT²Bank.SC2Bank",
			"CreatedBank.SC2Bank",
			"é🚀.SC2Bank",
			".hidden.SC2Bank",
		] {
			assert!(validate(OsStr::new(name)).is_ok(), "ordinary {name:?}");
		}
	}

	#[test]
	fn accepts_and_preserves_unpaired_surrogates_in_native_names() {
		// The high surrogate's low byte must not be interpreted as the DOS digit '1'.
		let name_units = [
			b'c' as u16,
			b'o' as u16,
			b'm' as u16,
			0xd831,
			b'.' as u16,
			0xdfff,
		];
		let name = OsString::from_wide(&name_units);
		assert!(
			name.to_str().is_none(),
			"fixture must contain unpaired surrogates"
		);
		assert!(validate(&name).is_ok(), "native name must remain usable");
		assert_eq!(name.encode_wide().collect::<Vec<_>>(), name_units);
	}

	#[test]
	fn caps_native_leaf_length_in_code_units() {
		let allowed = OsString::from_wide(&[0xd800; MAX_LEAF_UNITS]);
		let too_long = OsString::from_wide(&[0xd800; MAX_LEAF_UNITS + 1]);
		assert!(validate(&allowed).is_ok(), "255 raw units must fit");
		assert!(
			validate(&too_long).is_err(),
			"256 raw units must be rejected"
		);
	}
}
