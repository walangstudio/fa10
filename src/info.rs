//! The `info` operation: inspect a `.fa10` file's metadata without restoring.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::format::{self, Footer, FOOTER_FIXED, HEADER_MAGIC, TRAILER_LEN};

/// Metadata describing a `.fa10` file.
#[derive(Debug, Clone)]
pub struct Fa10Info {
    pub path: PathBuf,
    pub total_size: u64,
    pub original_size: u64,
    pub original_filename: String,
    pub padding_size: u64,
    pub footer_size: u64,
    pub multiplier: f64,
    pub sha256: [u8; 32],
}

impl Fa10Info {
    pub fn sha256_hex(&self) -> String {
        hex(&self.sha256)
    }
}

/// Read and summarize a `.fa10` file. Validates header, footer magic, and CRC32.
pub fn info(path: &Path) -> Result<Fa10Info> {
    let file = File::open(path)?;
    let total_size = file.metadata()?.len();
    let mut reader = BufReader::new(file);

    format::check_header(&mut reader)?;
    let footer = Footer::read_from(&mut reader, total_size)?;

    let footer_size = footer.encoded_len() + TRAILER_LEN;
    let overhead = HEADER_MAGIC.len() as u64
        + FOOTER_FIXED
        + footer.original_filename.len() as u64
        + TRAILER_LEN;
    let padding_size = total_size.saturating_sub(overhead + footer.original_size);
    let multiplier = if footer.original_size > 0 {
        total_size as f64 / footer.original_size as f64
    } else {
        f64::INFINITY
    };

    Ok(Fa10Info {
        path: path.to_path_buf(),
        total_size,
        original_size: footer.original_size,
        original_filename: footer.original_filename,
        padding_size,
        footer_size,
        multiplier,
        sha256: footer.sha256,
    })
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
