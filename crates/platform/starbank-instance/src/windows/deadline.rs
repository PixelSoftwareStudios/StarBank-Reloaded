#![forbid(unsafe_code)]

use std::{future::poll_fn, io, pin::pin, task::Poll};

use tokio::time::{Instant, sleep_until};

pub(super) async fn before_deadline<T>(
	deadline: Instant,
	operation: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
	let mut timer = pin!(sleep_until(deadline));
	let mut operation = pin!(operation);
	poll_fn(|context| {
		// Let an expired timer win even when the I/O is already ready to make progress.
		if timer.as_mut().poll(context).is_ready() || Instant::now() >= deadline {
			return Poll::Ready(Err(deadline_error()));
		}
		let result = operation.as_mut().poll(context);
		if Instant::now() >= deadline {
			return Poll::Ready(Err(deadline_error()));
		}
		result
	})
	.await
}

pub(super) fn deadline_error() -> io::Error {
	io::Error::new(io::ErrorKind::TimedOut, "instance pipe deadline elapsed")
}

#[cfg(test)]
mod tests {
	use std::cell::Cell;

	use super::*;

	#[test]
	fn expired_deadline_does_not_poll_a_ready_operation() {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.expect("deadline test runtime")
			.block_on(async {
				let was_polled = Cell::new(false);
				let ready = poll_fn(|_| {
					was_polled.set(true);
					Poll::Ready(Ok(()))
				});
				let error = before_deadline(Instant::now(), ready)
					.await
					.expect_err("an expired deadline must win");
				assert_eq!(error.kind(), io::ErrorKind::TimedOut);
				assert!(!was_polled.get());
			});
	}
}
