use std::{
	hint::black_box,
	time::{Duration, Instant},
};

const WARMUP_COUNT: usize = 5;
const SAMPLE_COUNT: usize = 25;
const TAIL_PERCENTAGE: usize = 95;
const FULL_PERCENTAGE: usize = 100;

/// Prints a tab-separated header for one benchmark executable's measurements.
pub fn print_header() {
	println!("operation\tfixture\tinput_bytes\tsamples\tmedian_ns\tp95_ns");
}

/// Times one operation per sample and verifies every result after its timer stops.
///
/// Setup, verification, and destruction of the result and state are outside the
/// measured interval. Percentiles use the nearest-rank method.
pub fn measure<State, Output>(
	operation: &str,
	fixture: &str,
	input_bytes: usize,
	mut setup: impl FnMut() -> State,
	mut execute: impl FnMut(&mut State) -> Output,
	mut verify: impl FnMut(&State, Output),
) {
	let mut sample = || {
		let mut state = setup();
		let start = Instant::now();
		let output = black_box(execute(black_box(&mut state)));
		let elapsed = start.elapsed();
		verify(&state, output);
		elapsed
	};
	for _ in 0..WARMUP_COUNT {
		black_box(sample());
	}
	let mut elapsed = [Duration::ZERO; SAMPLE_COUNT];
	elapsed.fill_with(sample);
	elapsed.sort_unstable();
	let median = elapsed[SAMPLE_COUNT / 2];
	let tail = elapsed[(SAMPLE_COUNT * TAIL_PERCENTAGE).div_ceil(FULL_PERCENTAGE) - 1];
	println!(
		"{operation}\t{fixture}\t{input_bytes}\t{SAMPLE_COUNT}\t{}\t{}",
		median.as_nanos(),
		tail.as_nanos()
	);
}
