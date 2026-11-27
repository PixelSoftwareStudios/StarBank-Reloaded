#[cfg(feature = "native-stormlib")]
mod artifact;

fn main() -> std::process::ExitCode {
	#[cfg(feature = "native-stormlib")]
	if let Err(error) = artifact::link() {
		eprintln!("StormLib backend: {error}");
		return std::process::ExitCode::FAILURE;
	}
	std::process::ExitCode::SUCCESS
}
