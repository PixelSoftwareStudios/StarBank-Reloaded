use std::ffi::OsStr;
use std::path::Path;

use syn::Item;

const MAX_SOURCE_LINES: usize = 500;

/// Reports project layout rule violations in an authored Rust source file.
#[must_use]
pub(crate) fn check_source(path: &Path, source: &str) -> Vec<String> {
	let mut errors = Vec::new();
	let lines = source.lines().count();
	if lines > MAX_SOURCE_LINES {
		errors.push(format!("{lines} lines; limit {MAX_SOURCE_LINES}"));
	}

	let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
		return errors;
	};
	let entrypoint = file_name == "main.rs";
	if !matches!(file_name, "lib.rs" | "mod.rs") && !entrypoint {
		return errors;
	}

	match syn::parse_file(source) {
		Ok(file) => {
			let disallowed = if entrypoint {
				file.items.iter().any(|item| match item {
					Item::Fn(function) => function.sig.ident != "main",
					other => !is_structural(other),
				})
			} else {
				file.items.iter().any(|item| !is_structural(item))
			};
			if disallowed {
				errors.push("move non-structural items out of this module".into());
			}
		}
		Err(error) => errors.push(format!("cannot parse Rust source: {error}")),
	}
	errors
}

fn is_structural(item: &Item) -> bool {
	match item {
		Item::Use(_) | Item::ExternCrate(_) => true,
		Item::Mod(module) => module
			.content
			.as_ref()
			.is_none_or(|(_, items)| items.iter().all(is_structural)),
		_ => false,
	}
}

#[cfg(test)]
mod tests {
	use super::check_source;
	use std::path::Path;

	#[test]
	fn accepts_docs_attributes_nested_wiring_and_multiline_reexports() {
		let source = r##"//! Root docs.

#![forbid(unsafe_code)]
#[cfg(feature = "example")]
mod example {
	#[cfg(target_os = "linux")]
	pub mod unix;
	pub use unix::{Event, State};
}

pub use example::{
	Event,
	State,
};
"##;
		assert!(check_source(Path::new("lib.rs"), source).is_empty());
	}

	#[test]
	fn rejects_cfg_gated_behavior_and_opaque_macros_in_roots() {
		for source in [
			"#[cfg(test)] mod tests { fn smoke() {} }",
			"include!(\"behavior.rs\");",
			"const VALUE: usize = 1;",
		] {
			assert_eq!(check_source(Path::new("mod.rs"), source).len(), 1);
		}
	}

	#[test]
	fn checks_line_limits_in_leaf_modules_and_entry_points() {
		assert_eq!(
			check_source(Path::new("leaf.rs"), &"\n".repeat(501)).len(),
			1
		);
		assert_eq!(
			check_source(Path::new("src/main.rs"), "fn helper() {}").len(),
			1
		);
		assert_eq!(
			check_source(Path::new("src/bin/check/main.rs"), "fn helper() {}").len(),
			1
		);
		assert!(check_source(Path::new("src/main.rs"), "fn main() {}").is_empty());
	}
}
