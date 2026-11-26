//! Replays fixture commands over the native revisioned editor protocol.

use std::io::{self, BufRead as _, Write as _};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use starbank_editor::Engine;

#[derive(Deserialize)]
struct Request {
	control: String,
	bytes: Option<Vec<u8>>,
}

#[derive(Serialize)]
struct Reply {
	control: Value,
	bytes: Option<Vec<u8>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut engine = Engine::default();
	let mut stdout = io::stdout().lock();
	for line in io::stdin().lock().lines() {
		let request: Request = serde_json::from_str(&line?)?;
		let reply = engine.dispatch(&request.control, request.bytes.as_deref());
		let response = Reply {
			control: serde_json::from_str(&reply.json)?,
			bytes: reply.bytes,
		};
		serde_json::to_writer(&mut stdout, &response)?;
		writeln!(stdout)?;
	}
	Ok(())
}
