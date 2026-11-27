use super::*;

#[test]
fn worker_preferences_never_exceed_half_the_machine_or_four_workers() {
	for (cpu_count, automatic) in [
		(0, 1),
		(1, 1),
		(2, 1),
		(3, 1),
		(4, 2),
		(7, 3),
		(8, 4),
		(256, 4),
	] {
		let available = NonZeroUsize::new(cpu_count);
		assert_eq!(worker_limit(available, None), automatic);
		for maximum in 1..=6 {
			assert_eq!(
				worker_limit(available, NonZeroUsize::new(maximum)),
				automatic.min(maximum)
			);
		}
	}
}
