//! Turn CLI inputs (files and directories) into a sorted, de-duplicated list of
//! archive entries. Directories are walked recursively; symlinks are followed
//! (their target content is stored as a regular file) with cycle detection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{Fa10Error, Result};
use crate::format::EntryKind;

/// Backstop against pathological nesting / symlink loops that escape the
/// canonical-path visited set (e.g. across distinct mount views).
const MAX_DEPTH: usize = 256;

/// One entry to be packed: where to read it from and how to record it.
#[derive(Debug, Clone)]
pub struct PendingEntry {
    pub kind: EntryKind,
    /// Relative, `/`-separated archive path.
    pub stored_path: String,
    /// Source path on disk (unused for `EmptyDir`).
    pub fs_path: PathBuf,
    pub size: u64,
}

/// Collect entries from the given inputs, sorted by archive path. Loose files
/// are stored under their base name; a directory `foo` contributes `foo/...`.
pub fn collect_entries(inputs: &[PathBuf]) -> Result<Vec<PendingEntry>> {
    if inputs.is_empty() {
        return Err(Fa10Error::NoInputs);
    }

    let mut out = Vec::new();
    for input in inputs {
        let meta = std::fs::metadata(input)?; // follows symlinks
        if meta.is_file() {
            let name = base_name(input);
            out.push(PendingEntry {
                kind: EntryKind::File,
                stored_path: name,
                fs_path: input.clone(),
                size: meta.len(),
            });
        } else if meta.is_dir() {
            let prefix = base_name(input);
            let mut visited = HashSet::new();
            walk(input, &prefix, &mut visited, 0, &mut out)?;
        } else {
            return Err(Fa10Error::BadFormat(format!(
                "{} is neither a file nor a directory",
                input.display()
            )));
        }
    }

    // Reject collisions, then sort for a deterministic archive layout.
    let mut seen = HashSet::new();
    for e in &out {
        if !seen.insert(e.stored_path.clone()) {
            return Err(Fa10Error::DuplicateEntry(e.stored_path.clone()));
        }
    }
    out.sort_by(|a, b| a.stored_path.cmp(&b.stored_path));
    Ok(out)
}

/// The archive path component for a top-level input: its final path component,
/// or empty when there is none (e.g. `.` or a filesystem root).
fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn join_stored(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn walk(
    dir: &Path,
    prefix: &str,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
    out: &mut Vec<PendingEntry>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Fa10Error::SymlinkCycle(dir.to_path_buf()));
    }
    // Break symlink cycles: skip a directory we've already entered (by its
    // canonical, link-resolved path).
    let canon = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(canon) {
        return Ok(());
    }

    let mut children = 0usize;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // metadata() follows symlinks; a broken link errors -> skip it.
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let stored = join_stored(prefix, &name);
        if meta.is_dir() {
            children += 1;
            walk(&path, &stored, visited, depth + 1, out)?;
        } else if meta.is_file() {
            children += 1;
            out.push(PendingEntry {
                kind: EntryKind::File,
                stored_path: stored,
                fs_path: path,
                size: meta.len(),
            });
        }
        // Other kinds (sockets, fifos, devices) are skipped.
    }

    // A directory that contributed nothing is recorded so it round-trips. The
    // path may be empty for a bare `.` input; skip that degenerate case.
    if children == 0 && !prefix.is_empty() {
        out.push(PendingEntry {
            kind: EntryKind::EmptyDir,
            stored_path: prefix.to_string(),
            fs_path: dir.to_path_buf(),
            size: 0,
        });
    }
    Ok(())
}
