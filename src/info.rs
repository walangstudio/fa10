//! The `info` operation: inspect a `.fa10` archive's manifest without extracting.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::format::{self, Entry, Manifest, HEADER_MAGIC, TRAILER_LEN};

/// Metadata describing a `.fa10` archive.
#[derive(Debug, Clone)]
pub struct Fa10Info {
    pub path: PathBuf,
    pub total_size: u64,
    pub payload_size: u64,
    pub padding_size: u64,
    pub manifest_size: u64,
    pub entry_count: usize,
    pub multiplier: f64,
    pub entries: Vec<Entry>,
}

/// Read and summarize a `.fa10` archive. Validates header, manifest magic, and CRC32.
pub fn info(path: &Path) -> Result<Fa10Info> {
    let file = File::open(path)?;
    let total_size = file.metadata()?.len();
    let mut reader = BufReader::new(file);

    format::check_header(&mut reader)?;
    let manifest = Manifest::read_from(&mut reader, total_size)?;

    let payload_size = manifest.payload_size();
    let manifest_len = manifest.encoded_len();
    let overhead = HEADER_MAGIC.len() as u64 + manifest_len + TRAILER_LEN;
    let padding_size = total_size.saturating_sub(overhead + payload_size);
    let multiplier = if payload_size > 0 {
        total_size as f64 / payload_size as f64
    } else {
        f64::INFINITY
    };

    Ok(Fa10Info {
        path: path.to_path_buf(),
        total_size,
        payload_size,
        padding_size,
        manifest_size: manifest_len + TRAILER_LEN,
        entry_count: manifest.entries.len(),
        multiplier,
        entries: manifest.entries,
    })
}
