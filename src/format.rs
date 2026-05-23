//! On-disk `.fa10` archive format: header, manifest, and trailer encode/decode.
//!
//! Byte layout (see README for the full diagram):
//!
//! ```text
//! [0]       8   header magic     "FA10ARC\0"
//! [8]       ..  entry contents, concatenated in manifest order
//! [..]      P   padding          repeating recognizable ASCII pattern
//! [..]      M   manifest         (magic, entry table, crc32)
//! [EOF-16]  16  trailer          end magic "FA10AEND" (8) + manifest_length u64 LE (8)
//! ```

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::error::{Fa10Error, Result};

/// Header magic written at the very start of every `.fa10` archive.
pub const HEADER_MAGIC: &[u8; 8] = b"FA10ARC\0";
/// Magic at the start of the manifest.
pub const MANIFEST_MAGIC: &[u8; 8] = b"FA10MANI";
/// Magic at the start of the fixed trailer.
pub const END_MAGIC: &[u8; 8] = b"FA10AEND";

/// Default, recognizable padding pattern.
pub const DEFAULT_PATTERN: &str = "FA10-PADDING-BLOCK-";

/// Length of the fixed trailer: end magic (8) + manifest_length u64 (8).
pub const TRAILER_LEN: u64 = 16;

/// Per-entry fixed overhead in the manifest, in bytes:
/// kind(1) + path_len(4) + size(8) + sha256(32) = 45 (excludes the path bytes).
const ENTRY_FIXED: u64 = 1 + 4 + 8 + 32;

/// Manifest fixed overhead: magic(8) + entry_count(4) + crc32(4) = 16.
const MANIFEST_FIXED: u64 = 8 + 4 + 4;

/// What an archive entry represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    EmptyDir,
}

impl EntryKind {
    fn to_byte(self) -> u8 {
        match self {
            EntryKind::File => 0,
            EntryKind::EmptyDir => 1,
        }
    }
    fn from_byte(b: u8) -> Result<EntryKind> {
        match b {
            0 => Ok(EntryKind::File),
            1 => Ok(EntryKind::EmptyDir),
            other => Err(Fa10Error::BadFormat(format!("unknown entry kind {other}"))),
        }
    }
}

/// One member of the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    /// Relative, `/`-separated archive path.
    pub path: String,
    /// Content length in bytes (0 for `EmptyDir`).
    pub size: u64,
    /// SHA-256 of the content (zeroed for `EmptyDir`).
    pub sha256: [u8; 32],
}

/// Parsed archive manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub entries: Vec<Entry>,
}

impl Manifest {
    /// Total size of the content region (sum of file entry sizes).
    pub fn payload_size(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .map(|e| e.size)
            .sum()
    }

    /// On-disk length of this manifest (excludes the fixed trailer).
    pub fn encoded_len(&self) -> u64 {
        MANIFEST_FIXED
            + self
                .entries
                .iter()
                .map(|e| ENTRY_FIXED + e.path.len() as u64)
                .sum::<u64>()
    }

    /// Serialize the manifest bytes (without the trailing trailer).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len() as usize);
        buf.extend_from_slice(MANIFEST_MAGIC);
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            let name = e.path.as_bytes();
            buf.push(e.kind.to_byte());
            buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
            buf.extend_from_slice(&e.size.to_le_bytes());
            buf.extend_from_slice(&e.sha256);
            buf.extend_from_slice(name);
        }
        let crc = crc32fast::hash(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    /// Write the manifest followed by the fixed trailer.
    pub fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let body = self.encode();
        w.write_all(&body)?;
        w.write_all(END_MAGIC)?;
        w.write_all(&(body.len() as u64).to_le_bytes())?;
        Ok(())
    }

    /// Decode a manifest from its raw bytes, verifying magic and CRC32.
    pub fn decode(bytes: &[u8]) -> Result<Manifest> {
        if bytes.len() < MANIFEST_FIXED as usize {
            return Err(Fa10Error::BadFormat("manifest too short".into()));
        }
        if &bytes[0..8] != MANIFEST_MAGIC {
            return Err(Fa10Error::BadFormat("missing manifest magic".into()));
        }
        let crc_at = bytes.len() - 4;

        // Reject an impossible entry count *before* allocating or hashing: the
        // count is attacker-controlled and CRC32 is forgeable, so we cannot let
        // it drive a `Vec::with_capacity`. Each entry needs at least
        // `ENTRY_FIXED` bytes, so the count cannot exceed the bytes available.
        let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let max_possible = (crc_at - 12) / ENTRY_FIXED as usize;
        if count > max_possible {
            return Err(Fa10Error::BadFormat(
                "manifest entry count out of range".into(),
            ));
        }

        let expected_crc = crc32fast::hash(&bytes[..crc_at]);
        let stored_crc = u32::from_le_bytes(bytes[crc_at..].try_into().unwrap());
        if expected_crc != stored_crc {
            return Err(Fa10Error::FooterCrcMismatch);
        }

        let mut pos = 12usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if pos + ENTRY_FIXED as usize > crc_at {
                return Err(Fa10Error::BadFormat("truncated manifest entry".into()));
            }
            let kind = EntryKind::from_byte(bytes[pos])?;
            let name_len = u32::from_le_bytes(bytes[pos + 1..pos + 5].try_into().unwrap()) as usize;
            let size = u64::from_le_bytes(bytes[pos + 5..pos + 13].try_into().unwrap());
            let mut sha256 = [0u8; 32];
            sha256.copy_from_slice(&bytes[pos + 13..pos + 45]);
            pos += ENTRY_FIXED as usize;
            if pos + name_len > crc_at {
                return Err(Fa10Error::BadFormat("manifest path overruns".into()));
            }
            let path = String::from_utf8(bytes[pos..pos + name_len].to_vec())
                .map_err(|_| Fa10Error::BadFilename)?;
            pos += name_len;
            entries.push(Entry {
                kind,
                path,
                size,
                sha256,
            });
        }
        if pos != crc_at {
            return Err(Fa10Error::BadFormat("trailing bytes in manifest".into()));
        }
        Ok(Manifest { entries })
    }

    /// Reverse-read the manifest from a seekable archive, given its total length.
    pub fn read_from<R: Read + Seek>(reader: &mut R, file_len: u64) -> Result<Manifest> {
        if file_len < HEADER_MAGIC.len() as u64 + TRAILER_LEN {
            return Err(Fa10Error::BadFormat(
                "file too short to be an fa10 archive".into(),
            ));
        }
        reader.seek(SeekFrom::Start(file_len - TRAILER_LEN))?;
        let mut trailer = [0u8; TRAILER_LEN as usize];
        reader.read_exact(&mut trailer)?;
        if &trailer[0..8] != END_MAGIC {
            return Err(Fa10Error::BadFormat("missing end magic".into()));
        }
        let manifest_len = u64::from_le_bytes(trailer[8..16].try_into().unwrap());
        // `file_len >= HEADER + TRAILER` is guaranteed above, so this subtraction
        // is safe and the comparison cannot overflow (unlike `manifest_len + TRAILER_LEN`).
        let max_manifest = file_len - TRAILER_LEN - HEADER_MAGIC.len() as u64;
        if manifest_len < MANIFEST_FIXED || manifest_len > max_manifest {
            return Err(Fa10Error::BadFormat("implausible manifest length".into()));
        }

        let manifest_start = file_len - TRAILER_LEN - manifest_len;
        reader.seek(SeekFrom::Start(manifest_start))?;
        let mut buf = vec![0u8; manifest_len as usize];
        reader.read_exact(&mut buf)?;
        Manifest::decode(&buf)
    }
}

/// Verify the 8-byte header magic at the start of a reader.
pub fn check_header<R: Read>(reader: &mut R) -> Result<()> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != HEADER_MAGIC {
        return Err(Fa10Error::BadFormat("missing FA10ARC header magic".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample() -> Manifest {
        Manifest {
            entries: vec![
                Entry {
                    kind: EntryKind::File,
                    path: "foo/réport.txt".to_string(),
                    size: 123,
                    sha256: [7u8; 32],
                },
                Entry {
                    kind: EntryKind::EmptyDir,
                    path: "foo/empty".to_string(),
                    size: 0,
                    sha256: [0u8; 32],
                },
            ],
        }
    }

    #[test]
    fn manifest_roundtrip_in_memory() {
        let m = sample();
        let encoded = m.encode();
        assert_eq!(encoded.len() as u64, m.encoded_len());
        assert_eq!(Manifest::decode(&encoded).unwrap(), m);
    }

    #[test]
    fn manifest_and_trailer_reverse_read() {
        let m = sample();
        let mut buf = Vec::new();
        buf.extend_from_slice(HEADER_MAGIC);
        buf.extend_from_slice(b"some original content here");
        buf.extend_from_slice(b"PADPADPADPAD");
        m.write_to(&mut buf).unwrap();

        let len = buf.len() as u64;
        let mut cursor = Cursor::new(buf);
        assert_eq!(Manifest::read_from(&mut cursor, len).unwrap(), m);
    }

    #[test]
    fn corrupt_crc_is_detected() {
        let mut encoded = sample().encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;
        match Manifest::decode(&encoded) {
            Err(Fa10Error::FooterCrcMismatch) => {}
            other => panic!("expected CRC mismatch, got {other:?}"),
        }
    }

    #[test]
    fn missing_magic_rejected() {
        let mut bytes = sample().encode();
        bytes[0] = b'X';
        assert!(Manifest::decode(&bytes).is_err());
    }

    #[test]
    fn payload_size_sums_only_files() {
        assert_eq!(sample().payload_size(), 123);
    }

    #[test]
    fn header_check_works() {
        let mut ok = Cursor::new(b"FA10ARC\0rest".to_vec());
        assert!(check_header(&mut ok).is_ok());
        let mut bad = Cursor::new(b"NOPE!!!!rest".to_vec());
        assert!(check_header(&mut bad).is_err());
    }
}
