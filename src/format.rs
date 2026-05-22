//! On-disk `.fa10` format: header, footer, and trailer encode/decode.
//!
//! Byte layout (see README for the full diagram):
//!
//! ```text
//! [0]      5   header magic   "FA10\x00"
//! [5]      N   original content (verbatim)
//! [5+N]    P   padding (repeating ASCII pattern)
//! ...      F   footer (variable, length F = FOOTER_FIXED + filename_len)
//! [EOF-16] 16  trailer: end magic "FA10END\x00" (8) + footer_length u64 LE (8)
//! ```

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::error::{Fa10Error, Result};

/// Header magic written at the very start of every `.fa10` file.
pub const HEADER_MAGIC: &[u8; 5] = b"FA10\x00";
/// Magic at the start of the footer.
pub const FOOTER_MAGIC: &[u8; 8] = b"FA10FOOT";
/// Magic at the start of the fixed trailer.
pub const END_MAGIC: &[u8; 8] = b"FA10END\x00";

/// Default, recognizable padding pattern.
pub const DEFAULT_PATTERN: &str = "FA10-PADDING-BLOCK-";

/// Length of the fixed trailer: end magic (8) + footer_length u64 (8).
pub const TRAILER_LEN: u64 = 16;

/// Fixed (filename-independent) portion of the footer, in bytes:
/// magic(8) + original_size(8) + filename_len(4) + sha256(32) + crc32(4) = 56.
pub const FOOTER_FIXED: u64 = 8 + 8 + 4 + 32 + 4;

/// Parsed footer metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    pub original_size: u64,
    pub original_filename: String,
    pub sha256: [u8; 32],
}

impl Footer {
    /// Total on-disk length of this footer (excludes the fixed trailer).
    pub fn encoded_len(&self) -> u64 {
        FOOTER_FIXED + self.original_filename.len() as u64
    }

    /// Serialize the footer bytes (without the trailing trailer).
    pub fn encode(&self) -> Vec<u8> {
        let name = self.original_filename.as_bytes();
        let mut buf = Vec::with_capacity(self.encoded_len() as usize);
        buf.extend_from_slice(FOOTER_MAGIC);
        buf.extend_from_slice(&self.original_size.to_le_bytes());
        buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&self.sha256);
        // CRC32 over everything written so far (the footer prefix).
        let crc = crc32fast::hash(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    /// Write the footer followed by the fixed trailer.
    pub fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let footer = self.encode();
        w.write_all(&footer)?;
        w.write_all(END_MAGIC)?;
        w.write_all(&(footer.len() as u64).to_le_bytes())?;
        Ok(())
    }

    /// Decode a footer from its raw bytes, verifying magic and CRC32.
    pub fn decode(bytes: &[u8]) -> Result<Footer> {
        if bytes.len() < FOOTER_FIXED as usize {
            return Err(Fa10Error::BadFormat("footer too short".into()));
        }
        if &bytes[0..8] != FOOTER_MAGIC {
            return Err(Fa10Error::BadFormat("missing footer magic".into()));
        }
        let original_size = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let name_len = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;

        let name_start = 20;
        let name_end = name_start + name_len;
        let sha_end = name_end + 32;
        let crc_end = sha_end + 4;
        if bytes.len() != crc_end {
            return Err(Fa10Error::BadFormat(format!(
                "footer length mismatch: expected {crc_end}, got {}",
                bytes.len()
            )));
        }

        let expected_crc = crc32fast::hash(&bytes[..sha_end]);
        let stored_crc = u32::from_le_bytes(bytes[sha_end..crc_end].try_into().unwrap());
        if expected_crc != stored_crc {
            return Err(Fa10Error::FooterCrcMismatch);
        }

        let original_filename = String::from_utf8(bytes[name_start..name_end].to_vec())
            .map_err(|_| Fa10Error::BadFilename)?;
        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(&bytes[name_end..sha_end]);

        Ok(Footer {
            original_size,
            original_filename,
            sha256,
        })
    }

    /// Reverse-read the footer from a seekable `.fa10` file, given its total length.
    pub fn read_from<R: Read + Seek>(reader: &mut R, file_len: u64) -> Result<Footer> {
        if file_len < HEADER_MAGIC.len() as u64 + TRAILER_LEN {
            return Err(Fa10Error::BadFormat("file too short to be .fa10".into()));
        }
        // Read the fixed trailer.
        reader.seek(SeekFrom::Start(file_len - TRAILER_LEN))?;
        let mut trailer = [0u8; TRAILER_LEN as usize];
        reader.read_exact(&mut trailer)?;
        if &trailer[0..8] != END_MAGIC {
            return Err(Fa10Error::BadFormat("missing end magic".into()));
        }
        let footer_len = u64::from_le_bytes(trailer[8..16].try_into().unwrap());
        if footer_len < FOOTER_FIXED || footer_len + TRAILER_LEN > file_len {
            return Err(Fa10Error::BadFormat("implausible footer length".into()));
        }

        let footer_start = file_len - TRAILER_LEN - footer_len;
        reader.seek(SeekFrom::Start(footer_start))?;
        let mut footer = vec![0u8; footer_len as usize];
        reader.read_exact(&mut footer)?;
        Footer::decode(&footer)
    }
}

/// Verify the 5-byte header magic at the start of a reader.
pub fn check_header<R: Read>(reader: &mut R) -> Result<()> {
    let mut magic = [0u8; 5];
    reader.read_exact(&mut magic)?;
    if &magic != HEADER_MAGIC {
        return Err(Fa10Error::BadFormat("missing FA10 header magic".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample_footer() -> Footer {
        Footer {
            original_size: 123_456,
            original_filename: "réport.txt".to_string(),
            sha256: [7u8; 32],
        }
    }

    #[test]
    fn footer_roundtrip_in_memory() {
        let footer = sample_footer();
        let encoded = footer.encode();
        assert_eq!(encoded.len() as u64, footer.encoded_len());
        let decoded = Footer::decode(&encoded).unwrap();
        assert_eq!(footer, decoded);
    }

    #[test]
    fn footer_and_trailer_reverse_read() {
        let footer = sample_footer();
        let mut buf = Vec::new();
        buf.extend_from_slice(HEADER_MAGIC);
        buf.extend_from_slice(b"some original content here");
        buf.extend_from_slice(b"PADPADPADPAD");
        footer.write_to(&mut buf).unwrap();

        let len = buf.len() as u64;
        let mut cursor = Cursor::new(buf);
        let read = Footer::read_from(&mut cursor, len).unwrap();
        assert_eq!(footer, read);
    }

    #[test]
    fn corrupt_crc_is_detected() {
        let footer = sample_footer();
        let mut encoded = footer.encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xFF;
        match Footer::decode(&encoded) {
            Err(Fa10Error::FooterCrcMismatch) => {}
            other => panic!("expected CRC mismatch, got {other:?}"),
        }
    }

    #[test]
    fn missing_magic_rejected() {
        let mut bytes = sample_footer().encode();
        bytes[0] = b'X';
        assert!(Footer::decode(&bytes).is_err());
    }

    #[test]
    fn header_check_works() {
        let mut ok = Cursor::new(b"FA10\x00rest".to_vec());
        assert!(check_header(&mut ok).is_ok());
        let mut bad = Cursor::new(b"NOPE!rest".to_vec());
        assert!(check_header(&mut bad).is_err());
    }
}
