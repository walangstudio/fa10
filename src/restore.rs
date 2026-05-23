//! The `restore` (extract) operation: recover the exact tree from a `.fa10`
//! archive.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Fa10Error, Result};
use crate::format::{self, EntryKind, Manifest, HEADER_MAGIC, TRAILER_LEN};
use crate::progress::Progress;
use crate::safety;

const COPY_BUF: usize = 4 * 1024 * 1024;

/// Options controlling a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub input: PathBuf,
    /// Extraction root. `None` means the current directory.
    pub output: Option<PathBuf>,
    /// Verify each entry's SHA-256 (default true).
    pub verify: bool,
    /// Allow overwriting existing files.
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
    pub root: PathBuf,
    pub entry_count: usize,
    pub payload_size: u64,
    pub verified: bool,
}

pub fn restore(opts: &RestoreOptions, progress: &dyn Progress) -> Result<RestoreOutcome> {
    safety::check_path_allowed(&opts.input)?;

    let file = File::open(&opts.input)?;
    let file_len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(COPY_BUF, file);

    format::check_header(&mut reader)?;
    let manifest = Manifest::read_from(&mut reader, file_len)?;
    check_content_region(&manifest, file_len)?;

    let root = opts.output.clone().unwrap_or_else(|| PathBuf::from("."));
    safety::check_path_allowed(&root)?;
    safety::check_free_space(&root, manifest.payload_size())?;

    progress.set_total(manifest.payload_size());

    // Content is laid out in manifest order, right after the 8-byte header.
    reader.seek(SeekFrom::Start(HEADER_MAGIC.len() as u64))?;
    let mut buf = vec![0u8; COPY_BUF];

    for entry in &manifest.entries {
        let target = safety::safe_extract_path(&root, &entry.path)?;

        if entry.kind == EntryKind::EmptyDir {
            fs::create_dir_all(&target)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.exists() && !opts.force {
            return Err(Fa10Error::OutputExists(target));
        }

        let out_file = File::create(&target)?;
        let mut writer = BufWriter::with_capacity(COPY_BUF, out_file);
        let mut hasher = opts.verify.then(Sha256::new);
        let mut remaining = entry.size;
        while remaining > 0 {
            let want = remaining.min(buf.len() as u64) as usize;
            let n = reader.read(&mut buf[..want])?;
            if n == 0 {
                return Err(Fa10Error::BadFormat(format!(
                    "unexpected EOF restoring {}",
                    entry.path
                )));
            }
            if let Some(h) = hasher.as_mut() {
                h.update(&buf[..n]);
            }
            writer.write_all(&buf[..n])?;
            remaining -= n as u64;
            progress.add(n as u64);
        }
        writer.flush()?;

        if let Some(h) = hasher {
            let got: [u8; 32] = h.finalize().into();
            if got != entry.sha256 {
                let _ = fs::remove_file(&target);
                return Err(Fa10Error::ContentHashMismatch);
            }
        }
    }
    progress.finish();

    Ok(RestoreOutcome {
        root,
        entry_count: manifest.entries.len(),
        payload_size: manifest.payload_size(),
        verified: opts.verify,
    })
}

/// Reject an archive whose declared entry sizes exceed the bytes actually
/// present between the header and the manifest. Without this, inflated sizes
/// would read padding/manifest bytes (or run off the end) as file content.
fn check_content_region(manifest: &Manifest, file_len: u64) -> Result<()> {
    let region = file_len
        .checked_sub(HEADER_MAGIC.len() as u64 + TRAILER_LEN + manifest.encoded_len())
        .ok_or_else(|| Fa10Error::BadFormat("archive smaller than its manifest".into()))?;
    if manifest.payload_size() > region {
        return Err(Fa10Error::BadFormat(
            "manifest entry sizes exceed the archive content region".into(),
        ));
    }
    Ok(())
}

/// Verify that every file entry's stored content matches its SHA-256, without
/// writing any output. Used by `grow --verify`.
pub fn verify_file(path: &Path) -> Result<()> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(COPY_BUF, file);

    format::check_header(&mut reader)?;
    let manifest = Manifest::read_from(&mut reader, file_len)?;
    check_content_region(&manifest, file_len)?;

    reader.seek(SeekFrom::Start(HEADER_MAGIC.len() as u64))?;
    let mut buf = vec![0u8; COPY_BUF];
    for entry in &manifest.entries {
        if entry.kind != EntryKind::File {
            continue;
        }
        let mut hasher = Sha256::new();
        let mut remaining = entry.size;
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
        if got != entry.sha256 {
            return Err(Fa10Error::ContentHashMismatch);
        }
    }
    Ok(())
}
