//! Native Linux desktop fallback opens the selected bank's containing directory.

use std::{path::Path, process::Command};

pub(super) const OPENER: &str = "/usr/bin/xdg-open";

pub(super) fn command(bank: &Path) -> Option<Command> {
	let folder = bank.parent().filter(|folder| folder.is_absolute())?;
	let mut command = Command::new(OPENER);
	command.arg(folder.as_os_str());
	Some(command)
}

#[cfg(test)]
mod tests {
	use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};

	use super::*;

	#[test]
	fn the_fixed_opener_receives_only_the_parent_as_one_argument() {
		let directory = Path::new("/tmp/-SC2 Cache é & \"folder\"");
		let command = command(&directory.join("-bank.SC2Bank")).expect("folder command");
		assert_eq!(command.get_program(), OsStr::new("/usr/bin/xdg-open"));
		assert_eq!(
			command.get_args().collect::<Vec<_>>(),
			[directory.as_os_str()]
		);
	}

	#[test]
	fn folder_arguments_preserve_native_path_bytes() {
		let folder = Path::new(OsStr::from_bytes(b"/tmp/Banks\xff"));
		let command = command(&folder.join("Source.SC2Bank")).expect("folder command");
		assert_eq!(command.get_args().collect::<Vec<_>>(), [folder.as_os_str()]);
	}

	#[test]
	fn relative_paths_and_paths_without_a_parent_are_refused() {
		for rejected in [
			"",
			"/",
			"--help",
			"relative/Bank.SC2Bank",
			"file:///tmp/Bank.SC2Bank",
		] {
			assert!(command(Path::new(rejected)).is_none());
		}
	}
}
