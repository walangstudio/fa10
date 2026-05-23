//! Command-line interface definition (clap derive).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "fa10",
    version,
    about = "Grow a file into a larger, fully-reversible test file with recognizable padding.",
    long_about = None,
    arg_required_else_help = true,
)]
pub struct Cli {
    /// Suppress the banner and progress output.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Print extra detail (hashes, sizes, paths).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Subcommand names (and the auto-generated `help`) recognized by the parser.
/// Used to decide when a bare `fa10 <file>` should imply `grow`.
pub const SUBCOMMANDS: &[&str] = &[
    "grow", "restore", "info", "cake", "feast", "buffet", "diet", "slim", "help",
];

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Grow a file by appending recognizable, reversible padding (default 2x).
    Grow(GrowArgs),

    /// Restore the original file from a .fa10 file.
    #[command(visible_aliases = ["diet", "slim"])]
    Restore(RestoreArgs),

    /// Show metadata about a .fa10 file without restoring it.
    Info(InfoArgs),

    /// Grow to 2x (themed alias for `grow --multiplier 2`).
    Cake(ThemedArgs),

    /// Grow to 5x (themed alias for `grow --multiplier 5`).
    Feast(ThemedArgs),

    /// Grow to 10x (themed alias for `grow --multiplier 10`).
    Buffet(ThemedArgs),
}

#[derive(Debug, Args)]
pub struct GrowArgs {
    /// Input file(s) to grow.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output size as a multiple of the original (default 2.0).
    #[arg(short, long, conflicts_with = "size")]
    pub multiplier: Option<f64>,

    /// Absolute target output size, e.g. 100MB, 2GiB (binary units).
    #[arg(short, long)]
    pub size: Option<String>,

    /// Output path (only valid with a single input file).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Padding pattern to repeat (default: "FA10-PADDING-BLOCK-").
    #[arg(long)]
    pub pattern: Option<String>,

    /// Overwrite the original in place (requires --confirm).
    #[arg(long)]
    pub in_place: bool,

    /// Confirm operations that exceed safety caps or modify in place.
    #[arg(long)]
    pub confirm: bool,

    /// Verify the written file by re-hashing its content.
    #[arg(long)]
    pub verify: bool,

    /// Allow operating on more than 100 input files.
    #[arg(long)]
    pub batch: bool,
}

#[derive(Debug, Args)]
pub struct ThemedArgs {
    /// Input file(s) to grow.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output path (only valid with a single input file).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Padding pattern to repeat (default: "FA10-PADDING-BLOCK-").
    #[arg(long)]
    pub pattern: Option<String>,

    /// Overwrite the original in place (requires --confirm).
    #[arg(long)]
    pub in_place: bool,

    /// Confirm operations that exceed safety caps or modify in place.
    #[arg(long)]
    pub confirm: bool,

    /// Verify the written file by re-hashing its content.
    #[arg(long)]
    pub verify: bool,

    /// Allow operating on more than 100 input files.
    #[arg(long)]
    pub batch: bool,
}

#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// .fa10 file(s) to restore.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// Output path (only valid with a single input file).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Skip SHA-256 verification of the recovered content.
    #[arg(long)]
    pub no_verify: bool,

    /// Overwrite the output file if it already exists.
    #[arg(long)]
    pub force: bool,

    /// Allow operating on more than 100 input files.
    #[arg(long)]
    pub batch: bool,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    /// .fa10 file to inspect.
    pub file: PathBuf,
}
