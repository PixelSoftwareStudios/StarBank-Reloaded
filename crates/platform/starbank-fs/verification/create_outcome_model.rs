//! Portable outcome model for an eventual verified no-replace publication.
//! This program does not inspect or mutate the filesystem.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fingerprint {
	volume: u64,
	file_id: [u8; 16],
	contents: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Name {
	Absent,
	File(Fingerprint),
	Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Publication {
	Succeeded,
	VerifiedNoReplaceCollision,
	IndeterminateFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parent {
	SameAndChecked,
	Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
	CommittedVerified,
	RejectedUnchanged,
	CommitUncertain,
}

fn classify(
	expected: Fingerprint,
	publication: Publication,
	parent: Parent,
	stage: Name,
	target: Name,
) -> Outcome {
	use Name::{Absent, File};
	use Outcome::{CommitUncertain, CommittedVerified, RejectedUnchanged};

	match (publication, parent, stage, target) {
		(Publication::Succeeded, Parent::SameAndChecked, Absent, File(published))
			if published == expected =>
		{
			CommittedVerified
		}
		(
			Publication::VerifiedNoReplaceCollision,
			Parent::SameAndChecked,
			File(pending),
			File(existing),
		) if pending == expected
			&& existing.volume == expected.volume
			&& existing.file_id != expected.file_id =>
		{
			RejectedUnchanged
		}
		_ => CommitUncertain,
	}
}

#[cfg(test)]
mod tests {
	use super::{Fingerprint, Name, Outcome, Parent, Publication, classify};

	const STAGE: Fingerprint = Fingerprint {
		volume: 1,
		file_id: [2; 16],
		contents: [3; 32],
	};
	const OTHER: Fingerprint = Fingerprint {
		file_id: [4; 16],
		..STAGE
	};
	const CROSS_VOLUME: Fingerprint = Fingerprint { volume: 2, ..STAGE };
	const MODIFIED: Fingerprint = Fingerprint {
		contents: [5; 32],
		..STAGE
	};

	#[test]
	fn successful_publication_requires_expected_identity_contents_and_name_binding() {
		let success = Publication::Succeeded;
		let checked = Parent::SameAndChecked;
		for (stage, target, verdict) in [
			(Name::Absent, Name::File(STAGE), Outcome::CommittedVerified),
			(Name::Absent, Name::File(OTHER), Outcome::CommitUncertain),
			(
				Name::Absent,
				Name::File(CROSS_VOLUME),
				Outcome::CommitUncertain,
			),
			(Name::Absent, Name::File(MODIFIED), Outcome::CommitUncertain),
			(
				Name::File(STAGE),
				Name::File(STAGE),
				Outcome::CommitUncertain,
			),
			(Name::Absent, Name::Absent, Outcome::CommitUncertain),
			(
				Name::Unverified,
				Name::File(STAGE),
				Outcome::CommitUncertain,
			),
		] {
			assert_eq!(classify(STAGE, success, checked, stage, target), verdict);
		}
	}

	#[test]
	fn explicit_collision_requires_owned_stage_and_distinct_destination() {
		let collision = Publication::VerifiedNoReplaceCollision;
		let checked = Parent::SameAndChecked;
		for (stage, target, verdict) in [
			(
				Name::File(STAGE),
				Name::File(OTHER),
				Outcome::RejectedUnchanged,
			),
			(
				Name::File(STAGE),
				Name::File(CROSS_VOLUME),
				Outcome::CommitUncertain,
			),
			(
				Name::File(STAGE),
				Name::File(MODIFIED),
				Outcome::CommitUncertain,
			),
			(Name::File(STAGE), Name::Absent, Outcome::CommitUncertain),
			(
				Name::File(STAGE),
				Name::Unverified,
				Outcome::CommitUncertain,
			),
			(
				Name::File(MODIFIED),
				Name::File(OTHER),
				Outcome::CommitUncertain,
			),
			(Name::Absent, Name::File(OTHER), Outcome::CommitUncertain),
		] {
			assert_eq!(classify(STAGE, collision, checked, stage, target), verdict);
		}
	}

	#[test]
	fn uncertain_system_result_or_unverified_parent_stays_uncertain_for_all_names() {
		let observations = [
			Name::Absent,
			Name::File(STAGE),
			Name::File(OTHER),
			Name::File(MODIFIED),
			Name::Unverified,
		];
		for stage in observations {
			for target in observations {
				assert_eq!(
					classify(
						STAGE,
						Publication::IndeterminateFailure,
						Parent::SameAndChecked,
						stage,
						target
					),
					Outcome::CommitUncertain,
				);
				for publication in [
					Publication::Succeeded,
					Publication::VerifiedNoReplaceCollision,
				] {
					assert_eq!(
						classify(STAGE, publication, Parent::Unverified, stage, target),
						Outcome::CommitUncertain,
					);
				}
			}
		}
	}
}
