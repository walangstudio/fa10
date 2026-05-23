//! The `grow` (pack) operation: bundle files and directories into one larger,
//! reversible `.fa10` archive.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::archive::{self, PendingEntry};
use crate::error::{Fa10Error, Result};
use crate::format::{Entry, EntryKind, Manifest, DEFAULT_PATTERN, HEADER_MAGIC, TRAILER_LEN};
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
    pub inputs: Vec<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Target,
    pub pattern: String,
    pub in_place: bool,
    pub confirm: bool,
    pub verify: bool,
    /// Allow more than `MAX_UNCONFIRMED_BATCH` file entries.
    pub batch: bool,
}

impl GrowOptions {
    pub fn new(inputs: impl Into<Vec<PathBuf>>, target: Target) -> Self {
        GrowOptions {
            inputs: inputs.into(),
            output: None,
            target,
            pattern: DEFAULT_PATTERN.to_string(),
            in_place: false,
            confirm: false,
            verify: false,
            batch: false,
        }
    }
}

/// Result of a successful grow.
#[derive(Debug, Clone)]
pub struct GrowOutcome {
    pub output_path: PathBuf,
    pub entry_count: usize,
    pub payload_size: u64,
    pub output_size: u64,
    pub padding_size: u64,
    /// True if the requested target was below the minimum and was bumped up.
    pub clamped: bool,
}

/// Default sibling output path: `<input>.fa10`.
fn default_output(input: &Path) -> PathBuf {
    let mut name = input.as_os_str().to_owned();
    name.push(".fa10");
    PathBuf::from(name)
}

fn resolve_output(opts: &GrowOptions) -> PathBuf {
    if let Some(o) = &opts.output {
        return o.clone();
    }
    if opts.in_place {
        return opts.inputs[0].clone();
    }
    match opts.inputs.len() {
        1 => default_output(&opts.inputs[0]),
        _ => PathBuf::from("archive.fa10"),
    }
}

pub fn grow(opts: &GrowOptions, progress: &dyn Progress) -> Result<GrowOutcome> {
    if opts.pattern.is_empty() {
        return Err(Fa10Error::EmptyPattern);
    }

    let entries = archive::collect_entries(&opts.inputs)?;
    let file_count = entries.iter().filter(|e| e.kind == EntryKind::File).count();
    safety::check_batch_limit(file_count, opts.batch)?;

    // In-place only makes sense for a single regular file.
    if opts.in_place {
        let single_file =
            opts.inputs.len() == 1 && entries.len() == 1 && entries[0].kind == EntryKind::File;
        if !single_file {
            return Err(Fa10Error::InPlaceNotSingleFile);
        }
        if !opts.confirm {
            return Err(Fa10Error::InPlaceNeedsConfirm);
        }
    }

    let payload_size: u64 = entries.iter().map(|e| e.size).sum();

    // The manifest length is fully determined by the entry paths/count (the
    // per-entry SHA-256 is a fixed 32 bytes), so we can size padding before we
    // have hashed anything.
    let manifest_len = manifest_skeleton(&entries).encoded_len();
    let overhead = HEADER_MAGIC.len() as u64 + manifest_len + TRAILER_LEN;
    let min_output = overhead + payload_size;

    let (mut output_size, mut clamped) = match opts.target {
        Target::Size(s) => (s, false),
        Target::Multiplier(m) => ((payload_size as f64 * m).round() as u64, false),
    };
    if output_size < min_output {
        match opts.target {
            Target::Size(_) => {
                return Err(Fa10Error::TargetTooSmall {
                    requested: output_size,
                    minimum: min_output,
                })
            }
            Target::Multiplier(_) => {
                output_size = min_output;
                clamped = true;
            }
        }
    }
    let padding_size = output_size - min_output;

    let output_path = resolve_output(opts);

    for input in &opts.inputs {
        safety::check_path_allowed(input)?;
    }
    safety::check_path_allowed(&output_path)?;
    safety::check_size_cap(output_size, opts.confirm)?;
    safety::check_free_space(&output_path, output_size)?;

    if !opts.in_place && output_path.exists() {
        return Err(Fa10Error::OutputExists(output_path));
    }

    let write_path: PathBuf = if opts.in_place {
        let mut tmp = output_path.as_os_str().to_owned();
        tmp.push(".fa10.tmp");
        PathBuf::from(tmp)
    } else {
        output_path.clone()
    };

    progress.set_total(output_size);

    // On any failure, remove the partial file we created (never the originals).
    if let Err(e) = write_archive(
        &entries,
        &write_path,
        padding_size,
        opts.pattern.as_bytes(),
        progress,
    ) {
        if !opts.inputs.iter().any(|p| p == &write_path) {
            let _ = fs::remove_file(&write_path);
        }
        return Err(e);
    }

    if opts.in_place {
        if let Err(e) = fs::rename(&write_path, &output_path) {
            let _ = fs::remove_file(&write_path);
            return Err(e.into());
        }
    }
    progress.finish();

    if opts.verify {
        if let Err(e) = restore::verify_file(&output_path) {
            if !opts.in_place {
                let _ = fs::remove_file(&output_path);
            }
            return Err(e);
        }
    }

    Ok(GrowOutcome {
        output_path,
        entry_count: entries.len(),
        payload_size,
        output_size,
        padding_size,
        clamped,
    })
}

/// A manifest with the right shape (paths/kinds/sizes) but zeroed hashes, used
/// only to compute the encoded length up front.
fn manifest_skeleton(entries: &[PendingEntry]) -> Manifest {
    Manifest {
        entries: entries
            .iter()
            .map(|e| Entry {
                kind: e.kind,
                path: e.stored_path.clone(),
                size: e.size,
                sha256: [0u8; 32],
            })
            .collect(),
    }
}

fn write_archive(
    entries: &[PendingEntry],
    write_path: &Path,
    padding_size: u64,
    pattern: &[u8],
    progress: &dyn Progress,
) -> Result<()> {
    let out_file = File::create(write_path)?;
    let mut writer = BufWriter::with_capacity(WRITE_BUF, out_file);

    writer.write_all(HEADER_MAGIC)?;
    progress.add(HEADER_MAGIC.len() as u64);

    let mut manifest = Manifest {
        entries: Vec::with_capacity(entries.len()),
    };
    let mut buf = vec![0u8; READ_BUF];
    for e in entries {
        let sha = if e.kind == EntryKind::File {
            stream_entry(&e.fs_path, e.size, &mut writer, &mut buf, progress)?
        } else {
            [0u8; 32]
        };
        manifest.entries.push(Entry {
            kind: e.kind,
            path: e.stored_path.clone(),
            size: e.size,
            sha256: sha,
        });
    }

    write_padding(&mut writer, pattern, padding_size, progress)?;

    let mut manifest_bytes = Vec::new();
    manifest.write_to(&mut manifest_bytes)?;
    writer.write_all(&manifest_bytes)?;
    progress.add(manifest_bytes.len() as u64);

    writer.flush()?;
    Ok(())
}

/// Stream one file's content into `writer`, returning its SHA-256. Errors if the
/// file changed size mid-read (TOCTOU).
fn stream_entry<W: Write>(
    path: &Path,
    expected: u64,
    writer: &mut W,
    buf: &mut [u8],
    progress: &dyn Progress,
) -> Result<[u8; 32]> {
    let mut reader = BufReader::with_capacity(READ_BUF, File::open(path)?);
    let mut hasher = Sha256::new();
    let mut copied = 0u64;
    loop {
        let n = reader.read(buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
        copied += n as u64;
        progress.add(n as u64);
    }
    if copied != expected {
        return Err(Fa10Error::BadFormat(format!(
            "{} changed during read: expected {expected} bytes, read {copied}",
            path.display()
        )));
    }
    Ok(hasher.finalize().into())
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
    let reps = (PADDING_BLOCK_TARGET / pattern.len()).max(1);
    let mut block = Vec::with_capacity(reps * pattern.len());
    for _ in 0..reps {
        block.extend_from_slice(pattern);
    }
    while remaining > 0 {
        let chunk = remaining.min(block.len() as u64) as usize;
        writer.write_all(&block[..chunk])?;
        remaining -= chunk as u64;
        progress.add(chunk as u64);
    }
    Ok(())
}
