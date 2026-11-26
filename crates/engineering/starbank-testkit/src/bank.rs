use std::fmt::Write;

const KEYS_PER_SECTION: usize = 256;

/// Small and large key counts shared by the editing and signing workloads.
pub const BANK_WORKLOADS: [(&str, usize); 2] = [("small", 32), ("large", 8_192)];

/// Builds a deterministic unsigned bank with one integer per uniquely named key.
#[must_use]
pub fn synthetic_bank(keys: usize) -> Vec<u8> {
	let mut bank = String::from("<Bank version=\"1\">\n");
	for (section, start) in (0..keys).step_by(KEYS_PER_SECTION).enumerate() {
		writeln!(bank, "  <Section name=\"section-{section:03}\">")
			.expect("writing fixture section to a String");
		let end = start.saturating_add(KEYS_PER_SECTION).min(keys);
		for key in (start..end).rev() {
			writeln!(
				bank,
				"    <Key name=\"key-{key:05}\"><Value int=\"{key:05}\"/></Key>"
			)
			.expect("writing fixture key to a String");
		}
		bank.push_str("  </Section>\n");
	}
	bank.push_str("</Bank>\n");
	bank.into_bytes()
}
