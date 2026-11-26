use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

pub(crate) const JSON_OPTION: &str = "json";
pub(crate) const JSON_FLAG: &str = "--json";

#[derive(Debug, Parser)]
#[command(name = "starbank", version, about)]
pub(crate) struct Cli {
	/// Write versioned JSON to standard output.
	#[arg(long = JSON_OPTION, global = true)]
	pub(crate) json: bool,

	#[command(subcommand)]
	pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
	/// Display bank contents and metadata.
	Inspect(BankArgs),
	/// Validate bank XML and report its signature state.
	Validate {
		#[command(flatten)]
		bank: BankArgs,

		/// Require a verified signature for this signing context.
		#[arg(long)]
		require_signature: bool,
	},
	/// Compare the ordered contents of two banks.
	Diff { before: PathBuf, after: PathBuf },
	/// Prepare a verified signature for a bank.
	Sign {
		#[command(flatten)]
		bank: BankArgs,

		#[command(flatten)]
		destination: DestinationArgs,
	},
	/// Apply exact, fingerprint-checked value edits.
	Edit {
		#[command(flatten)]
		bank: BankArgs,

		/// Versioned JSON patch file from the matching bank fingerprint.
		#[arg(long)]
		patch: PathBuf,

		#[command(flatten)]
		destination: DestinationArgs,

		#[command(flatten)]
		signing: OptionalSigningArgs,
	},
	/// Validate and export a bank to an absent destination.
	Export {
		#[command(flatten)]
		bank: BankArgs,

		/// Create this file exclusively.
		#[arg(long)]
		output: PathBuf,

		/// Produce the same prepared bytes and diff without writing.
		#[arg(long)]
		dry_run: bool,

		#[command(flatten)]
		signing: OptionalSigningArgs,
	},
	/// Create, verify and restore adjacent bank snapshots.
	Backup {
		#[command(subcommand)]
		command: BackupCommand,
	},
	/// Find local live and backup bank files.
	Discover {
		/// Search a selected bank, account folder, or bank collection.
		#[arg(long)]
		root: Option<PathBuf>,
	},
}

#[derive(Debug, Args)]
pub(crate) struct BankArgs {
	pub(crate) file: PathBuf,

	#[command(flatten)]
	pub(crate) context: ContextArgs,
}

#[derive(Debug, Default, Args)]
#[group(id = "explicit_context", multiple = true)]
pub(crate) struct ContextArgs {
	/// SC2 handle for the map author.
	#[arg(long, requires_all = ["player", "bank_name"])]
	pub(crate) author: Option<String>,

	/// SC2 handle for the player.
	#[arg(long, requires_all = ["author", "bank_name"])]
	pub(crate) player: Option<String>,

	/// Exact bank name used for signature computation.
	#[arg(long, requires_all = ["author", "player"])]
	pub(crate) bank_name: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct DestinationArgs {
	/// Preview the exact prepared operations and signature status without writing.
	#[arg(long)]
	pub(crate) dry_run: bool,

	/// Create an absent file exclusively.
	#[arg(long, conflicts_with = "in_place", required_unless_present_any = ["dry_run", "in_place"])]
	pub(crate) output: Option<PathBuf>,

	/// Replace the observed input only after a verified backup.
	#[arg(long)]
	pub(crate) in_place: bool,

	/// Mark the adjacent in-place backup as explicitly requested.
	#[arg(long, requires = "in_place")]
	pub(crate) backup: bool,
}

#[derive(Debug, Default, Args)]
pub(crate) struct OptionalSigningArgs {
	/// Preserve an existing signature even when a signing context is available.
	#[arg(long)]
	pub(crate) no_resign: bool,

	/// Authorize writing a preserved signature that cannot be verified.
	#[arg(long)]
	pub(crate) allow_stale_signature: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BackupCommand {
	/// List recorded adjacent backups for a bank.
	List {
		file: PathBuf,
		/// Read each recorded snapshot and check its stored length and digest.
		#[arg(long)]
		verify: bool,
	},
	/// Copy the observed bank bytes to a unique verified adjacent backup.
	Create { file: PathBuf },
	/// Restore verified backup bytes after creating a reversal snapshot.
	Restore {
		/// Live bank to replace.
		file: PathBuf,

		/// Exact snapshot path reported by backup list.
		backup: PathBuf,

		/// Current bank digest from a recent inspection.
		#[arg(long)]
		expected_current_sha256: String,

		/// Verify the restore plan without creating a reversal or replacing the bank.
		#[arg(long)]
		dry_run: bool,
	},
}
