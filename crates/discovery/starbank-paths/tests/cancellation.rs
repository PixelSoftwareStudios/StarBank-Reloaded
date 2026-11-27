use std::{fs, io, num::NonZeroUsize};

use starbank_paths::{
	LimitKind, PathError, ScanLimits, inventory_banks, inventory_banks_with_cancellation,
	is_valid_bank_path,
};

#[test]
fn cancels_inside_a_nested_bank_walk_before_reaching_its_entry_limit()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let banks = temp.path().join("Banks");
	let nested = banks.join("Nested");
	fs::create_dir_all(&nested)?;
	for name in ["First", "Second", "Third"] {
		fs::write(nested.join(format!("{name}.SC2Bank")), b"<Bank/>")?;
	}

	let limits = ScanLimits::default();
	let selection = is_valid_bank_path(&banks, limits)?;
	let bounded = ScanLimits::new(4, NonZeroUsize::new(2).unwrap());
	assert!(matches!(
		inventory_banks(&selection, bounded),
		Err(PathError::LimitReached {
			kind: LimitKind::Entries,
			..
		})
	));

	// A nested directory and one bank fit the budget. Cancel before reading
	// the next entry, which would exceed it.
	const CANCEL_BEFORE_SECOND_BANK: usize = 9;
	let mut polls = 0;
	let result = inventory_banks_with_cancellation(&selection, bounded, || {
		polls += 1;
		polls == CANCEL_BEFORE_SECOND_BANK
	});
	assert!(matches!(result, Err(PathError::Cancelled)));
	assert_eq!(polls, CANCEL_BEFORE_SECOND_BANK);

	let complete = inventory_banks(&selection, limits)?;
	assert_eq!(complete.banks.len(), 3);
	assert_eq!(
		inventory_banks_with_cancellation(&selection, limits, || false)?,
		complete
	);
	Ok(())
}

#[test]
fn cancelled_single_file_selection_stops_before_reading_its_metadata()
-> Result<(), Box<dyn std::error::Error>> {
	let temp = tempfile::tempdir()?;
	let path = temp.path().join("Removed.SC2Bank");
	fs::write(&path, b"<Bank/>")?;
	let limits = ScanLimits::default();
	let selection = is_valid_bank_path(&path, limits)?;
	fs::remove_file(path)?;

	assert!(matches!(
		inventory_banks_with_cancellation(&selection, limits, || true),
		Err(PathError::Cancelled)
	));
	assert!(matches!(
		inventory_banks(&selection, limits),
		Err(PathError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound
	));
	Ok(())
}
