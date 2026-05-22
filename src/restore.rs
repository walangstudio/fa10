//! The `restore` operation: recover the exact original from a `.fa10` file.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Fa10Error, Result};
use crate::format::{self, Footer, HEADER_MAGIC};
use crate::progress::Progress;
use crate::safety;

const COPY_BUF: usize = 4 * 1024 * 1024;

/// Options controlling a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    /// Verify SHA-256 of the recovered content (default true).
    pub verify: bool,
    /// Allow overwriting an existing output file.
    pub force: bool,
}

impl RestoreOptions {
    pub fn new(input: impl Into<PathBuf>) -> Self {
        RestoreOptions {
            input: input.into(),
            output: None,
            verify: true,
            force: false,
        }
    }
}

/// Result of a successful restore.
#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    pub output_path: PathBuf,
    pub original_size: u64,
    pub verified: bool,
}

/// Choose where to write the recovered original.
fn resolve_output(input: &Path, footer: &Footer, explicit: &Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p.clone();
    }
    // Prefer the recorded original filename, placed alongside the input.
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    if !footer.original_filename.is_empty() {
        return parent.join(&footer.original_filename);
    }
    // Fall back to stripping a trailing `.fa10`.
    if input.extension().and_then(|e| e.to_str()) == Some("fa10") {
        return input.with_extension("");
    }
    parent.join("restored.bin")
}

pub fn restore(opts: &RestoreOptions, progress: &dyn Progress) -> Result<RestoreOutcome> {
    safety::check_path_allowed(&opts.input)?;

    let file = File::open(&opts.input)?;
    let file_len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(COPY_BUF, file);

    // Validate header.
    format::check_header(&mut reader)?;

    // Reverse-read the footer.
    let footer = Footer::read_from(&mut reader, file_len)?;

    let output_path = resolve_output(&opts.input, &footer, &opts.output);
    safety::check_path_allowed(&output_path)?;
    if output_path.exists() && !opts.force {
        return Err(Fa10Error::OutputExists(output_path));
    }
    safety::check_free_space(&output_path, footer.original_size)?;

    progress.set_total(footer.original_size);

    // Stream the content region [5, 5 + original_size) to the output.
    reader.seek(SeekFrom::Start(HEADER_MAGIC.len() as u64))?;
    let out_file = File::create(&output_path)?;
    let mut writer = BufWriter::with_capacity(COPY_BUF, out_file);

    let mut hasher = Sha256::new();
    let mut remaining = footer.original_size;
    let mut buf = vec![0u8; COPY_BUF];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = reader.read(&mut buf[..want])?;
        if n == 0 {
            return Err(Fa10Error::BadFormat(
                "unexpected EOF while reading original content".into(),
            ));
        }
        if opts.verify {
            hasher.update(&buf[..n]);
        }
        writer.write_all(&buf[..n])?;
        remaining -= n as u64;
        progress.add(n as u64);
    }
    writer.flush()?;
    progress.finish();

    let verified = if opts.verify {
        let got: [u8; 32] = hasher.finalize().into();
        if got != footer.sha256 {
            // Remove the bad output so we don't leave a corrupt file behind.
            let _ = fs::remove_file(&output_path);
            return Err(Fa10Error::ContentHashMismatch);
        }
        true
    } else {
        false
    };

    Ok(RestoreOutcome {
        output_path,
        original_size: footer.original_size,
        verified,
    })
}

/// Verify that a `.fa10` file's stored content matches its footer SHA-256,
/// without writing any output. Used by `grow --verify`.
pub fn verify_file(path: &Path) -> Result<()> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(COPY_BUF, file);

    format::check_header(&mut reader)?;
    let footer = Footer::read_from(&mut reader, file_len)?;

    reader.seek(SeekFrom::Start(HEADER_MAGIC.len() as u64))?;
    let mut hasher = Sha256::new();
    let mut remaining = footer.original_size;
    let mut buf = vec![0u8; COPY_BUF];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = reader.read(&mut buf[..want])?;
        if n == 0 {
            return Err(Fa10Error::BadFormat("truncated content region".into()));
        }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    let got: [u8; 32] = hasher.finalize().into();
    if got != footer.sha256 {
        return Err(Fa10Error::ContentHashMismatch);
    }
    Ok(())
}
