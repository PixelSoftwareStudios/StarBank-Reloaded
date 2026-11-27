use std::env;
use std::error::Error;
use std::fmt::Write;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

const ARTIFACT_DIRECTORY: &str = "STARBANK_STORMLIB_ARTIFACT";
const ARCHIVE_NAME: &str = "libstorm.a";
const PROVENANCE_NAME: &str = "provenance";
const STAMP_VERSION: &str = "starbank-stormlib-bootstrap-v1";
const SOURCE_SHA256: &str = "f80b08bae168888702a4251312d6c2279f487673df611c2b5ef4f395e810c3b1";
const SUPPORTED_TARGET: &str = "aarch64-apple-darwin";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const ARCHIVE_MAGIC: &[u8] = b"!<arch>\n";

pub(super) fn link() -> Result<(), Box<dyn Error>> {
	let target = env::var("TARGET")?;
	let host = env::var("HOST")?;
	if target != SUPPORTED_TARGET || host != SUPPORTED_TARGET {
		return Err(
			format!("native-stormlib requires an {SUPPORTED_TARGET} host and target").into(),
		);
	}
	println!("cargo::rerun-if-env-changed={ARTIFACT_DIRECTORY}");
	let directory = env::var_os(ARTIFACT_DIRECTORY)
		.map(PathBuf::from)
		.ok_or("set STARBANK_STORMLIB_ARTIFACT using bootstrap-macos-arm64.sh")?;
	directory
		.to_str()
		.filter(|path| !path.contains(['\n', '\r']))
		.ok_or("the artifact directory must be UTF-8 without line separators")?;
	if !directory.is_absolute() {
		return Err("the artifact directory must be an absolute path".into());
	}
	let archive_path = directory.join(ARCHIVE_NAME);
	let provenance_path = directory.join(PROVENANCE_NAME);
	println!("cargo::rerun-if-changed={}", archive_path.display());
	println!("cargo::rerun-if-changed={}", provenance_path.display());

	let archive = fs::File::open(archive_path)?;
	if !archive.metadata()?.is_file() {
		return Err("expected a regular static archive".into());
	}
	let mut bytes = Vec::new();
	let read = archive
		.take(MAX_ARCHIVE_BYTES + 1)
		.read_to_end(&mut bytes)?;
	if u64::try_from(read)? > MAX_ARCHIVE_BYTES {
		return Err("StormLib archive exceeds the artifact size limit".into());
	}
	if !bytes.starts_with(ARCHIVE_MAGIC) {
		return Err("expected an archive with Ar magic".into());
	}
	let mut archive_sha256 = String::new();
	for byte in Sha256::digest(&bytes) {
		write!(archive_sha256, "{byte:02x}")?;
	}
	let expected = format!(
		"{STAMP_VERSION}\nsource={SOURCE_SHA256}\ntarget={SUPPORTED_TARGET}\narchive={archive_sha256}\n"
	);
	let mut provenance = String::new();
	fs::File::open(provenance_path)?
		.take(u64::try_from(expected.len())? + 1)
		.read_to_string(&mut provenance)?;
	if provenance != expected {
		return Err(
			"StormLib source, target or archive checksum differs from the bootstrap stamp".into(),
		);
	}
	// Bind the linker to the bytes just authenticated, under a name that cannot be
	// shadowed by a generic system libstorm archive.
	let out_dir = env::var("OUT_DIR")?;
	if out_dir.contains(['\n', '\r']) {
		return Err("the Cargo output directory contains a line separator".into());
	}
	let library = format!("starbank_stormlib_9_40_{archive_sha256}");
	fs::write(
		PathBuf::from(&out_dir).join(format!("lib{library}.a")),
		bytes,
	)?;
	println!("cargo::rustc-link-search=native={out_dir}");
	println!("cargo::rustc-link-lib=static={library}");
	println!("cargo::rustc-link-lib=dylib=c++");
	Ok(())
}
