//! Command-line interface definition (clap derive).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "fa10",
    version,
    about = "Inflate files and directories into one larger, fully-reversible .fa10 archive (the opposite of zip).",
    long_about = None,
    arg_required_else_help = true,
)]
pub struct Cli {
    /// Suppress progress and result output.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Print extra detail (hashes, sizes, paths).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Subcommand names and aliases (plus the auto-generated `help`) recognized by
/// the parser. Used to decide when a bare `fa10 <file>` should imply `inflate`.
pub const SUBCOMMANDS: &[&str] = &[
    "inflate", "grow", "restore", "info", "cake", "feast", "buffet", "diet", "slim", "help",
];

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Inflate files/directories into a reversible .fa10 archive (the default action, 2x).
    #[command(alias = "grow")]
    Inflate(GrowArgs),

    /// Extract a .fa10 archive, recreating its tree.
    #[command(visible_aliases = ["diet", "slim"])]
    Restore(RestoreArgs),

    /// List a .fa10 archive's entries and metadata without extracting it.
    Info(InfoArgs),

    /// Inflate to 2x (themed alias for `--multiplier 2`).
    Cake(ThemedArgs),

    /// Inflate to 5x (themed alias for `--multiplier 5`).
    Feast(ThemedArgs),

    /// Inflate to 10x (themed alias for `--multiplier 10`).
    Buffet(ThemedArgs),
}

#[derive(Debug, Args)]
pub struct GrowArgs {
    /// Files and/or directories to pack into one archive.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output size as a multiple of the total input size (default 2.0).
    #[arg(short, long, conflicts_with = "size")]
    pub multiplier: Option<f64>,

    /// Absolute target output size, e.g. 100MB, 2GiB (binary units).
    #[arg(short, long)]
    pub size: Option<String>,

    /// Output archive path (default: <input>.fa10, or archive.fa10 for 2+ inputs).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Padding pattern to repeat (default: "FA10-PADDING-BLOCK-").
    #[arg(long)]
    pub pattern: Option<String>,

    /// Replace a single input file with its archive in place (requires --confirm).
    #[arg(long)]
    pub in_place: bool,

    /// Confirm operations that exceed safety caps or modify in place.
    #[arg(long)]
    pub confirm: bool,

    /// Verify the archive by re-hashing every entry after writing.
    #[arg(long)]
    pub verify: bool,

    /// Allow packing more than 100 files.
    #[arg(long)]
    pub batch: bool,
}

#[derive(Debug, Args)]
pub struct ThemedArgs {
    /// Files and/or directories to pack into one archive.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output archive path (default: <input>.fa10, or archive.fa10 for 2+ inputs).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Padding pattern to repeat (default: "FA10-PADDING-BLOCK-").
    #[arg(long)]
    pub pattern: Option<String>,

    /// Replace a single input file with its archive in place (requires --confirm).
    #[arg(long)]
    pub in_place: bool,

    /// Confirm operations that exceed safety caps or modify in place.
    #[arg(long)]
    pub confirm: bool,

    /// Verify the archive by re-hashing every entry after writing.
    #[arg(long)]
    pub verify: bool,

    /// Allow packing more than 100 files.
    #[arg(long)]
    pub batch: bool,
}

#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// Archive(s) to extract.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Directory to extract into (default: current directory).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Skip SHA-256 verification of the extracted content.
    #[arg(long)]
    pub no_verify: bool,

    /// Overwrite existing files when extracting.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    /// Archive to inspect.
    pub file: PathBuf,
}
