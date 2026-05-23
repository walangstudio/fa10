use std::path::PathBuf;
use thiserror::Error;

/// Errors produced by the `fa10` library.
#[derive(Debug, Error)]
pub enum Fa10Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not a valid .fa10 file: {0}")]
    BadFormat(String),

    #[error("footer integrity check failed (CRC32 mismatch)")]
    FooterCrcMismatch,

    #[error("content verification failed: SHA-256 mismatch (file is corrupt or truncated)")]
    ContentHashMismatch,

    #[error("requested target size {requested} bytes is too small; minimum is {minimum} bytes")]
    TargetTooSmall { requested: u64, minimum: u64 },

    #[error("invalid size string {input:?}: {reason}")]
    BadSize { input: String, reason: String },

    #[error("padding pattern must not be empty")]
    EmptyPattern,

    #[error("refusing to operate on protected system path: {0}")]
    ProtectedPath(PathBuf),

    #[error("not enough free disk space: operation would leave less than {min_free} bytes free (needs {needed} bytes, {available} available)")]
    InsufficientSpace {
        needed: u64,
        available: u64,
        min_free: u64,
    },

    #[error(
        "output size {size} bytes exceeds the {cap} byte safety cap; pass --confirm to proceed"
    )]
    SizeCapExceeded { size: u64, cap: u64 },

    #[error("batch of {count} files exceeds the {limit} file limit; pass --batch to proceed")]
    BatchLimitExceeded { count: usize, limit: usize },

    #[error("output path already exists: {0} (use --output to choose another path, or remove it)")]
    OutputExists(PathBuf),

    #[error("in-place operation requires --confirm")]
    InPlaceNeedsConfirm,

    #[error("filename in footer is not valid UTF-8")]
    BadFilename,

    #[error("no input files or directories given")]
    NoInputs,

    #[error("--in-place only works on a single file (not a directory or multiple inputs)")]
    InPlaceNotSingleFile,

    #[error("two inputs map to the same archive path {0:?}; rename one or pack them separately")]
    DuplicateEntry(String),

    #[error("symlink loop detected while walking {0} (or directory nesting too deep)")]
    SymlinkCycle(PathBuf),

    #[error("refusing to extract unsafe archive path {0:?} (absolute, parent-escaping, or drive-qualified)")]
    UnsafeEntryPath(String),

    #[error("{0} is not an fa10 archive (bad header magic)")]
    NotAnArchive(PathBuf),
}

pub type Result<T> = std::result::Result<T, Fa10Error>;
