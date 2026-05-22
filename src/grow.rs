//! The `grow` operation: turn a file into a larger, reversible `.fa10` file.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Fa10Error, Result};
use crate::format::{Footer, DEFAULT_PATTERN, FOOTER_FIXED, HEADER_MAGIC, TRAILER_LEN};
use crate::progress::Progress;
use crate::{restore, safety};

const READ_BUF: usize = 4 * 1024 * 1024;
const WRITE_BUF: usize = 4 * 1024 * 1024;
const PADDING_BLOCK_TARGET: usize = 64 * 1024;

/// How the target output size is requested.
#[derive(Debug, Clone)]
pub enum Target {
    Multiplier(f64),
    Size(u64),
}

/// Options controlling a grow operation.
#[derive(Debug, Clone)]
pub struct GrowOptions {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub target: Target,
    pub pattern: String,
    pub in_place: bool,
    pub confirm: bool,
    pub verify: bool,
}

impl GrowOptions {
    pub fn new(input: impl Into<PathBuf>, target: Target) -> Self {
        GrowOptions {
            input: input.into(),
            output: None,
            target,
            pattern: DEFAULT_PATTERN.to_string(),
            in_place: false,
            confirm: false,
            verify: false,
        }
    }
}

/// Result of a successful grow.
#[derive(Debug, Clone)]
pub struct GrowOutcome {
    pub output_path: PathBuf,
    pub original_size: u64,
    pub output_size: u64,
    pub padding_size: u64,
    pub sha256: [u8; 32],
    /// True if the requested target was below the minimum and was bumped up.
    pub clamped: bool,
}

/// Compute the byte overhead (header + footer + trailer) for a given filename.
fn overhead_for(filename: &str) -> u64 {
    HEADER_MAGIC.len() as u64 + FOOTER_FIXED + filename.len() as u64 + TRAILER_LEN
}

/// Default sibling output path: `<input>.fa10`.
fn default_output(input: &Path) -> PathBuf {
    let mut name = input.as_os_str().to_owned();
    name.push(".fa10");
    PathBuf::from(name)
}

pub fn grow(opts: &GrowOptions, progress: &dyn Progress) -> Result<GrowOutcome> {
    if opts.pattern.is_empty() {
        return Err(Fa10Error::EmptyPattern);
    }

    let meta = fs::metadata(&opts.input)?;
    if !meta.is_file() {
        return Err(Fa10Error::BadFormat(format!(
            "{} is not a regular file",
            opts.input.display()
        )));
    }
    let original_size = meta.len();
    let filename = opts
        .input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());

    let overhead = overhead_for(&filename);
    let min_output = overhead + original_size;

    let (mut output_size, mut clamped) = match opts.target {
        Target::Size(s) => (s, false),
        Target::Multiplier(m) => {
            let scaled = (original_size as f64 * m).round() as u64;
            (scaled, false)
        }
    };
    if output_size < min_output {
        match opts.target {
            // Explicit byte target below the minimum is an error.
            Target::Size(_) => {
                return Err(Fa10Error::TargetTooSmall {
                    requested: output_size,
                    minimum: min_output,
                })
            }
            // Multiplier on a tiny file: bump up to the minimum and flag it.
            Target::Multiplier(_) => {
                output_size = min_output;
                clamped = true;
            }
        }
    }
    let padding_size = output_size - min_output;

    // Resolve output path.
    let output_path = if opts.in_place {
        if !opts.confirm {
            return Err(Fa10Error::InPlaceNeedsConfirm);
        }
        opts.input.clone()
    } else {
        opts.output
            .clone()
            .unwrap_or_else(|| default_output(&opts.input))
    };

    // Safety checks.
    safety::check_path_allowed(&opts.input)?;
    safety::check_path_allowed(&output_path)?;
    safety::check_size_cap(output_size, opts.confirm)?;
    safety::check_free_space(&output_path, output_size)?;

    if !opts.in_place && output_path.exists() {
        return Err(Fa10Error::OutputExists(output_path));
    }

    // When growing in place, write to a temp sibling then atomically rename.
    let write_path: PathBuf = if opts.in_place {
        let mut tmp = output_path.as_os_str().to_owned();
        tmp.push(".fa10.tmp");
        PathBuf::from(tmp)
    } else {
        output_path.clone()
    };

    progress.set_total(output_size);

    let sha = write_fa10(
        &opts.input,
        &write_path,
        original_size,
        &filename,
        padding_size,
        opts.pattern.as_bytes(),
        progress,
    )?;

    if opts.in_place {
        fs::rename(&write_path, &output_path)?;
    }
    progress.finish();

    if opts.verify {
        restore::verify_file(&output_path)?;
    }

    Ok(GrowOutcome {
        output_path,
        original_size,
        output_size,
        padding_size,
        sha256: sha,
        clamped,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_fa10(
    input: &Path,
    write_path: &Path,
    original_size: u64,
    filename: &str,
    padding_size: u64,
    pattern: &[u8],
    progress: &dyn Progress,
) -> Result<[u8; 32]> {
    let in_file = File::open(input)?;
    let mut reader = BufReader::with_capacity(READ_BUF, in_file);
    let out_file = File::create(write_path)?;
    let mut writer = BufWriter::with_capacity(WRITE_BUF, out_file);

    // Header.
    writer.write_all(HEADER_MAGIC)?;
    progress.add(HEADER_MAGIC.len() as u64);

    // Stream original content while hashing.
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; READ_BUF];
    let mut copied = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
        copied += n as u64;
        progress.add(n as u64);
    }
    if copied != original_size {
        return Err(Fa10Error::BadFormat(format!(
            "input changed during read: expected {original_size} bytes, read {copied}"
        )));
    }
    let sha: [u8; 32] = hasher.finalize().into();

    // Padding: write a pattern-aligned block buffer repeatedly.
    write_padding(&mut writer, pattern, padding_size, progress)?;

    // Footer + trailer.
    let footer = Footer {
        original_size,
        original_filename: filename.to_string(),
        sha256: sha,
    };
    let mut footer_bytes = Vec::new();
    footer.write_to(&mut footer_bytes)?;
    writer.write_all(&footer_bytes)?;
    progress.add(footer_bytes.len() as u64);

    writer.flush()?;
    Ok(sha)
}

fn write_padding<W: Write>(
    writer: &mut W,
    pattern: &[u8],
    mut remaining: u64,
    progress: &dyn Progress,
) -> Result<()> {
    if remaining == 0 {
        return Ok(());
    }
    // Build a block that is a whole number of patterns, ~PADDING_BLOCK_TARGET.
    let reps = (PADDING_BLOCK_TARGET / pattern.len()).max(1);
    let mut block = Vec::with_capacity(reps * pattern.len());
    for _ in 0..reps {
        block.extend_from_slice(pattern);
    }
    // Because block.len() is a multiple of pattern.len(), each full block ends
    // on a pattern boundary, so writing the first `rem` bytes of `block` for the
    // final partial chunk keeps the repeating pattern phase continuous.
    while remaining > 0 {
        let chunk = remaining.min(block.len() as u64) as usize;
        writer.write_all(&block[..chunk])?;
        remaining -= chunk as u64;
        progress.add(chunk as u64);
    }
    Ok(())
}
