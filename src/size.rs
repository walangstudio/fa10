//! Human-readable size parsing. All units are binary (1024-based).
//!
//! Accepted forms (case-insensitive, optional whitespace):
//!   - bare integer: bytes (e.g. `1048576`)
//!   - `K`/`KB`/`KiB`, `M`/`MB`/`MiB`, `G`/`GB`/`GiB`, `T`/`TB`/`TiB`
//!
//! Per project decision, the `*B` family (`MB`, `GB`) is treated as 1024-based
//! and is equivalent to the `*iB` family (`MiB`, `GiB`).

use crate::error::{Fa10Error, Result};

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;
const TIB: u64 = GIB * 1024;

/// Parse a human-readable size string into a byte count (binary units).
pub fn parse_size(input: &str) -> Result<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(bad(input, "empty size"));
    }

    // Split leading numeric portion from the unit suffix.
    let split = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '_'))
        .unwrap_or(trimmed.len());
    let (num_part, unit_part) = trimmed.split_at(split);
    let unit = unit_part.trim().to_ascii_lowercase();

    let multiplier = match unit.as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => KIB,
        "m" | "mb" | "mib" => MIB,
        "g" | "gb" | "gib" => GIB,
        "t" | "tb" | "tib" => TIB,
        other => return Err(bad(input, &format!("unknown unit {other:?}"))),
    };

    let clean: String = num_part.chars().filter(|&c| c != '_').collect();
    if clean.is_empty() {
        return Err(bad(input, "missing number"));
    }

    let bytes = if clean.contains('.') {
        let value: f64 = clean
            .parse()
            .map_err(|_| bad(input, "invalid decimal number"))?;
        if !value.is_finite() || value < 0.0 {
            return Err(bad(input, "number must be non-negative and finite"));
        }
        (value * multiplier as f64).round() as u64
    } else {
        let value: u64 = clean
            .parse()
            .map_err(|_| bad(input, "invalid integer (too large?)"))?;
        value
            .checked_mul(multiplier)
            .ok_or_else(|| bad(input, "size overflows u64"))?
    };

    if bytes == 0 {
        return Err(bad(input, "size must be greater than zero"));
    }
    Ok(bytes)
}

fn bad(input: &str, reason: &str) -> Fa10Error {
    Fa10Error::BadSize {
        input: input.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_bytes() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1").unwrap(), 1);
    }

    #[test]
    fn binary_units() {
        assert_eq!(parse_size("1K").unwrap(), 1024);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("1KiB").unwrap(), 1024);
        assert_eq!(parse_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("100MB").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_size("2GiB").unwrap(), 2 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1TB").unwrap(), 1024u64.pow(4));
    }

    #[test]
    fn case_and_whitespace_insensitive() {
        assert_eq!(parse_size("  5 mb ").unwrap(), 5 * 1024 * 1024);
        assert_eq!(parse_size("5Mb").unwrap(), 5 * 1024 * 1024);
    }

    #[test]
    fn decimal_values() {
        assert_eq!(parse_size("1.5KB").unwrap(), 1536);
        assert_eq!(parse_size("0.5MB").unwrap(), 512 * 1024);
    }

    #[test]
    fn underscores_allowed() {
        assert_eq!(parse_size("1_000").unwrap(), 1000);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_size("").is_err());
        assert!(parse_size("abc").is_err());
        assert!(parse_size("10XB").is_err());
        assert!(parse_size("0").is_err());
        assert!(parse_size("MB").is_err());
    }
}
