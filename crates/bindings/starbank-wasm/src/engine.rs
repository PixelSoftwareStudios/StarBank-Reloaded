use starbank_editor::{Engine, MAX_CONTROL_BYTES, PROTOCOL_VERSION, max_attachment_bytes};
use wasm_bindgen::prelude::wasm_bindgen;

/// Owns one application engine in a browser worker.
#[derive(Default)]
#[wasm_bindgen]
pub struct WasmBankEngine {
	engine: Engine,
}

#[wasm_bindgen]
impl WasmBankEngine {
	/// Creates a worker-local application engine.
	#[wasm_bindgen(constructor)]
	pub fn new() -> Self {
		Self::default()
	}

	/// Reports the version of editor command and response envelopes.
	#[wasm_bindgen(getter)]
	pub fn schema_version(&self) -> u32 {
		PROTOCOL_VERSION
	}

	/// Reports the maximum bank attachment length in bytes.
	#[wasm_bindgen(getter)]
	pub fn max_bank_bytes(&self) -> usize {
		max_attachment_bytes()
	}

	/// Reports the maximum request JSON length in UTF-8 bytes.
	#[wasm_bindgen(getter)]
	pub fn max_control_bytes(&self) -> usize {
		MAX_CONTROL_BYTES
	}

	/// Dispatches a revisioned command with an optional separate byte attachment.
	pub fn dispatch(&mut self, control: &str, bytes: Option<Vec<u8>>) -> WasmReply {
		let reply = self.engine.dispatch(control, bytes.as_deref());
		WasmReply {
			control: reply.json,
			bytes: reply.bytes,
		}
	}
}

/// Owns the response data until the worker hands it to the client.
#[wasm_bindgen]
pub struct WasmReply {
	control: String,
	bytes: Option<Vec<u8>>,
}

#[wasm_bindgen]
impl WasmReply {
	/// Takes the JSON response, leaving the stored value empty.
	pub fn take_control(&mut self) -> String {
		std::mem::take(&mut self.control)
	}

	/// Takes the optional binary attachment.
	pub fn take_bytes(&mut self) -> Option<Vec<u8>> {
		self.bytes.take()
	}
}
