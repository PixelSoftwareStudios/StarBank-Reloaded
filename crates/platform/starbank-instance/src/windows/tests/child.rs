use std::process::{Child, Command};

use super::*;

pub(super) struct QualificationChild(Child);

impl QualificationChild {
	pub(super) fn pid(&self) -> u32 {
		self.0.id()
	}

	pub(super) fn launch(name: &str, case: &str) -> Self {
		let child = Command::new(std::env::current_exe().expect("test executable"))
			.args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
			.env(CHILD_PARENT_PID, process::id().to_string())
			.env(CHILD_NAMESPACE, name)
			.env(CHILD_CASE, case)
			.spawn()
			.expect("start test child");
		assert_ne!(child.id(), process::id());
		Self(child)
	}

	pub(super) async fn verify_exit(mut self) {
		loop {
			if let Some(status) = self.0.try_wait().expect("child exit status") {
				assert!(status.success(), "qualification child exited with {status}");
				return;
			}
			sleep_until(Instant::now() + RETRY_INTERVAL).await;
		}
	}
}

impl Drop for QualificationChild {
	fn drop(&mut self) {
		if self.0.kill().is_ok() {
			let _ = self.0.wait();
		}
	}
}
