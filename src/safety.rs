//! Good-citizen guardrails: protected-path blocklist, free-space floor,
//! output-size cap, and batch limits. These prevent footguns; they are not a
//! security sandbox (see SECURITY.md).

use std::path::{Path, PathBuf};

use crate::error::{Fa10Error, Result};

/// Minimum free space that must remain after an operation (2 GiB).
pub const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Largest output an unconfirmed operation may produce (10 GiB).
pub const MAX_UNCONFIRMED_OUTPUT: u64 = 10 * 1024 * 1024 * 1024;

/// Largest batch (file count) allowed without `--batch`.
pub const MAX_UNCONFIRMED_BATCH: usize = 100;

/// Absolute protected path prefixes (Unix + Windows).
const ABSOLUTE_BLOCKLIST: &[&str] = &[
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/boot",
    "/etc",
    "/dev",
    "/proc",
    "/sys",
    "/lib",
    "/Library",
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
];

/// Home-relative protected path suffixes (joined to the user's home dir).
const HOME_RELATIVE_BLOCKLIST: &[&str] = &["Library"];

/// Build the full list of protected path prefixes for this machine.
fn protected_prefixes() -> Vec<PathBuf> {
    let mut prefixes: Vec<PathBuf> = ABSOLUTE_BLOCKLIST.iter().map(PathBuf::from).collect();
    if let Some(home) = home_dir() {
        for suffix in HOME_RELATIVE_BLOCKLIST {
            prefixes.push(home.join(suffix));
        }
    }
    prefixes
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Best-effort absolute-path normalization that does not require the path to
/// exist (so we can also guard not-yet-created output files).
fn normalize(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path)
    } else {
        path.to_path_buf()
    };
    // Resolve symlinks/`..` where possible; fall back to the lexical path.
    abs.canonicalize().unwrap_or_else(|_| {
        // Canonicalize the nearest existing ancestor, then re-append the rest.
        let mut existing = abs.clone();
        let mut tail = Vec::new();
        while !existing.exists() {
            match existing.file_name() {
                Some(name) => {
                    tail.push(name.to_owned());
                    if !existing.pop() {
                        break;
                    }
                }
                None => break,
            }
        }
        let mut base = existing.canonicalize().unwrap_or(existing);
        for part in tail.into_iter().rev() {
            base.push(part);
        }
        base
    })
}

/// Refuse to operate on protected system locations.
pub fn check_path_allowed(path: &Path) -> Result<()> {
    let normalized = normalize(path);
    for prefix in protected_prefixes() {
        let prefix_norm = normalize(&prefix);
        if normalized == prefix_norm || normalized.starts_with(&prefix_norm) {
            return Err(Fa10Error::ProtectedPath(path.to_path_buf()));
        }
    }
    Ok(())
}

/// Ensure writing `needed` bytes near `target` leaves at least `MIN_FREE_BYTES`.
pub fn check_free_space(target: &Path, needed: u64) -> Result<()> {
    // Query the nearest existing ancestor directory.
    let mut probe = target.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            probe = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            break;
        }
    }
    let available = fs2::available_space(&probe)?;
    if available < needed.saturating_add(MIN_FREE_BYTES) {
        return Err(Fa10Error::InsufficientSpace {
            needed,
            available,
            min_free: MIN_FREE_BYTES,
        });
    }
    Ok(())
}

/// Enforce the unconfirmed output-size cap.
pub fn check_size_cap(output_size: u64, confirmed: bool) -> Result<()> {
    if !confirmed && output_size > MAX_UNCONFIRMED_OUTPUT {
        return Err(Fa10Error::SizeCapExceeded {
            size: output_size,
            cap: MAX_UNCONFIRMED_OUTPUT,
        });
    }
    Ok(())
}

/// Resolve a stored archive path against an extraction `root`, refusing any
/// path that could escape it (absolute, parent-relative, or drive-qualified).
/// This is the Zip-Slip guard.
pub fn safe_extract_path(root: &Path, stored: &str) -> Result<PathBuf> {
    let unsafe_path = || Fa10Error::UnsafeEntryPath(stored.to_string());
    if stored.is_empty() || stored.starts_with('/') {
        return Err(unsafe_path());
    }
    // `/`-separated by format contract; reject any back-slashes outright.
    if stored.contains('\\') || stored.contains('\0') {
        return Err(unsafe_path());
    }
    let mut path = root.to_path_buf();
    for comp in stored.split('/') {
        match comp {
            "" | "." => continue,
            ".." => return Err(unsafe_path()),
            // A drive/scheme-qualified component like `C:` or a Windows root.
            c if c.contains(':') => return Err(unsafe_path()),
            c => path.push(c),
        }
    }
    // Defense in depth: the lexical join must stay under the root.
    if !path.starts_with(root) {
        return Err(unsafe_path());
    }
    Ok(path)
}

/// Enforce the unconfirmed batch limit.
pub fn check_batch_limit(count: usize, batch_ok: bool) -> Result<()> {
    if !batch_ok && count > MAX_UNCONFIRMED_BATCH {
        return Err(Fa10Error::BatchLimitExceeded {
            count,
            limit: MAX_UNCONFIRMED_BATCH,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_system_paths() {
        assert!(check_path_allowed(Path::new("/etc/passwd")).is_err());
        assert!(check_path_allowed(Path::new("/usr/bin/whatever")).is_err());
        #[cfg(windows)]
        assert!(check_path_allowed(Path::new("C:\\Windows\\system32")).is_err());
    }

    #[test]
    fn allows_normal_paths() {
        let tmp = std::env::temp_dir().join("fa10_safety_probe.txt");
        assert!(check_path_allowed(&tmp).is_ok());
    }

    #[test]
    fn size_cap_logic() {
        assert!(check_size_cap(MAX_UNCONFIRMED_OUTPUT, false).is_ok());
        assert!(check_size_cap(MAX_UNCONFIRMED_OUTPUT + 1, false).is_err());
        assert!(check_size_cap(MAX_UNCONFIRMED_OUTPUT + 1, true).is_ok());
    }

    #[test]
    fn batch_limit_logic() {
        assert!(check_batch_limit(MAX_UNCONFIRMED_BATCH, false).is_ok());
        assert!(check_batch_limit(MAX_UNCONFIRMED_BATCH + 1, false).is_err());
        assert!(check_batch_limit(MAX_UNCONFIRMED_BATCH + 1, true).is_ok());
    }

    #[test]
    fn safe_extract_path_allows_nested() {
        let root = Path::new("/tmp/out");
        assert_eq!(
            safe_extract_path(root, "foo/bar/baz.txt").unwrap(),
            root.join("foo").join("bar").join("baz.txt")
        );
        // Redundant `.` and leading slash collapse safely.
        assert_eq!(
            safe_extract_path(root, "foo/./baz.txt").unwrap(),
            root.join("foo").join("baz.txt")
        );
    }

    #[test]
    fn safe_extract_path_rejects_escapes() {
        let root = Path::new("/tmp/out");
        for bad in [
            "../evil",
            "foo/../../evil",
            "/etc/passwd",
            "C:\\Windows\\system32",
            "foo\\..\\bar",
            "",
        ] {
            assert!(
                safe_extract_path(root, bad).is_err(),
                "should reject {bad:?}"
            );
        }
    }
}
