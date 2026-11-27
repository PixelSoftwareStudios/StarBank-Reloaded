//! Keep each successful fill step consistent with its byte-derived iteration budget.

use super::{FILL_CHUNK, FILL_TARGET_FREE, FillPolicy, MAX_CAPACITY, MIB};

#[derive(Debug, Eq, PartialEq)]
pub(super) enum FillStep {
	Complete,
	RestoreHeadroom,
	Write(usize),
	RefuseBudget,
}

pub(super) fn next_step(policy: FillPolicy, available: usize, filled: usize) -> FillStep {
	if policy == FillPolicy::ReportedThreshold && available <= FILL_TARGET_FREE {
		return if filled > 0 && available >= MIB {
			FillStep::Complete
		} else {
			FillStep::RestoreHeadroom
		};
	}
	if filled
		.checked_add(FILL_CHUNK)
		.is_none_or(|next| next > MAX_CAPACITY)
	{
		return FillStep::RefuseBudget;
	}
	// A small f_bavail gap does not promise a new allocation per small append. APFS
	// may already have allocated extent slack; short writes would invalidate the
	// full-chunk-derived observation budget without making comparable progress.
	FillStep::Write(FILL_CHUNK)
}

#[test]
fn a_one_byte_reported_gap_still_requests_a_full_bounded_chunk() {
	assert_eq!(
		next_step(FillPolicy::ReportedThreshold, FILL_TARGET_FREE + 1, MIB),
		FillStep::Write(FILL_CHUNK)
	);
}

#[test]
fn deferred_extent_accounting_reaches_threshold_within_the_existing_budget() {
	const ALLOCATED_EXTENT_SLACK: usize = 8 * MIB;
	let mut slack = ALLOCATED_EXTENT_SLACK;
	let mut filled = 0;
	for observation in 1..=super::MAX_FILL_STEPS {
		// A deterministic counter that does not change until already-allocated
		// slack is consumed. This models the permitted lag, not an APFS measurement.
		let available = FILL_TARGET_FREE + usize::from(slack != 0);
		match next_step(FillPolicy::ReportedThreshold, available, filled) {
			FillStep::Complete => {
				assert_eq!(filled, ALLOCATED_EXTENT_SLACK);
				assert_eq!(observation, ALLOCATED_EXTENT_SLACK / FILL_CHUNK + 1);
				return;
			}
			FillStep::Write(count) => {
				filled += count;
				slack = slack.saturating_sub(count);
				assert!(filled <= MAX_CAPACITY);
			}
			step => panic!("unexpected deferred-allocation step: {step:?}"),
		}
	}
	panic!("deferred allocation exhausted the observation budget");
}

#[test]
fn permanently_stale_above_threshold_counter_refuses_at_the_original_byte_cap() {
	let mut filled = 0;
	for observation in 1..=super::MAX_FILL_STEPS {
		match next_step(FillPolicy::ReportedThreshold, FILL_TARGET_FREE + 1, filled) {
			FillStep::Write(count) => {
				filled += count;
				assert!(filled <= MAX_CAPACITY);
			}
			FillStep::RefuseBudget => {
				assert_eq!(filled, MAX_CAPACITY);
				assert_eq!(observation, super::MAX_FILL_STEPS);
				return;
			}
			step => panic!("unexpected stale-counter step: {step:?}"),
		}
	}
	panic!("the capacity cap must refuse before another write");
}

#[test]
fn threshold_stop_and_forced_exhaustion_remain_distinct() {
	assert_eq!(
		next_step(FillPolicy::ReportedThreshold, FILL_TARGET_FREE, MIB),
		FillStep::Complete
	);
	assert_eq!(
		next_step(FillPolicy::UntilEnospc, FILL_TARGET_FREE, MIB),
		FillStep::Write(FILL_CHUNK)
	);
	assert_eq!(
		next_step(FillPolicy::ReportedThreshold, 0, MIB),
		FillStep::RestoreHeadroom
	);
	assert_eq!(
		next_step(FillPolicy::UntilEnospc, 0, MIB),
		FillStep::Write(FILL_CHUNK)
	);
}

#[test]
fn a_threshold_reached_on_the_last_allowed_write_is_observed_before_refusal() {
	assert_eq!(
		next_step(
			FillPolicy::ReportedThreshold,
			FILL_TARGET_FREE,
			MAX_CAPACITY
		),
		FillStep::Complete
	);
	assert_eq!(
		next_step(FillPolicy::UntilEnospc, FILL_TARGET_FREE, MAX_CAPACITY),
		FillStep::RefuseBudget
	);
}
