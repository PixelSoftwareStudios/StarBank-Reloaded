use starbank_mpq::{V1_APPEND_ARCHIVE_LIMIT, V1AppendPlan};

use crate::AppendError;
use crate::native::write_members;
use crate::scratch::Scratch;

/// Build and independently verify a private archive candidate for a prepared append plan.
///
/// Only the adapter-generated scratch file is passed through the native ABI. The returned bytes
/// passed the append preservation checks. Component semantics and publication belong to the
/// caller.
///
/// # Errors
///
/// Returns an error if private local APFS scratch is unavailable, any native operation fails,
/// or independently checking the resulting archive fails.
pub fn append(plan: &V1AppendPlan<'_>) -> Result<Vec<u8>, AppendError> {
	let mut scratch = Scratch::create(plan.source())?;
	let candidate = write_members(&scratch, plan.additions())
		.and_then(|()| scratch.read_candidate(V1_APPEND_ARCHIVE_LIMIT));
	let cleanup = scratch.remove();
	let candidate = match (candidate, cleanup) {
		(Ok(candidate), Ok(())) => candidate,
		(Err(primary), Err(cleanup)) => {
			return Err(AppendError::Cleanup {
				primary: Box::new(primary),
				cleanup: Box::new(cleanup),
			});
		}
		(Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
	};
	plan.verify(&candidate).map_err(AppendError::Verification)?;
	Ok(candidate)
}
