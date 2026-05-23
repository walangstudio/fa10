use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use fa10::format::{Entry, EntryKind, Manifest, END_MAGIC, HEADER_MAGIC};
use fa10::grow::{GrowOptions, Target};
use fa10::progress::NoProgress;
use fa10::restore::RestoreOptions;
use fa10::{grow, info, restore};

/// Deterministic pseudo-random bytes so the tests are reproducible.
fn pseudo_random(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut state: u64 = 0x9E3779B97F4A7C15;
    for _ in 0..len {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        out.push((state >> 33) as u8);
    }
    out
}

/// Read a directory tree into `relative/slash/path -> bytes` (files only).
fn read_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn single_file_roundtrips_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("payload.bin");
    let data = pseudo_random(4096);
    fs::write(&original, &data).unwrap();

    let opts = GrowOptions::new(vec![original.clone()], Target::Multiplier(3.0));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    assert_eq!(outcome.output_path, dir.path().join("payload.bin.fa10"));
    assert_eq!(outcome.entry_count, 1);
    assert_eq!(outcome.payload_size, data.len() as u64);
    assert_eq!(outcome.output_size, data.len() as u64 * 3);

    let bytes = fs::read(&outcome.output_path).unwrap();
    assert_eq!(bytes.len() as u64, outcome.output_size);
    assert_eq!(&bytes[..8], HEADER_MAGIC);
    assert_eq!(&bytes[bytes.len() - 16..bytes.len() - 8], END_MAGIC);
    assert!(bytes
        .windows(b"FA10-PADDING-BLOCK-".len())
        .any(|w| w == b"FA10-PADDING-BLOCK-"));

    let meta = info::info(&outcome.output_path).unwrap();
    assert_eq!(meta.entry_count, 1);
    assert_eq!(meta.payload_size, data.len() as u64);
    assert_eq!(meta.entries[0].path, "payload.bin");
    assert!((meta.multiplier - 3.0).abs() < 1e-9);

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(outcome.output_path.clone());
    ropts.output = Some(out.path().to_path_buf());
    let r = restore::restore(&ropts, &NoProgress).unwrap();
    assert!(r.verified);
    assert_eq!(fs::read(out.path().join("payload.bin")).unwrap(), data);
}

#[test]
fn directory_tree_roundtrips() {
    let src = tempfile::tempdir().unwrap();
    let root = src.path().join("data");
    fs::create_dir_all(root.join("sub/deep")).unwrap();
    fs::write(root.join("a.txt"), b"alpha").unwrap();
    fs::write(root.join("sub/b.bin"), pseudo_random(3000)).unwrap();
    fs::write(root.join("sub/deep/c.txt"), b"").unwrap(); // empty file
    fs::create_dir(root.join("emptydir")).unwrap(); // empty directory

    let opts = GrowOptions::new(vec![root.clone()], Target::Multiplier(2.0));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();
    assert_eq!(outcome.output_path, src.path().join("data.fa10"));
    assert!(outcome.entry_count >= 3);

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(outcome.output_path);
    ropts.output = Some(out.path().to_path_buf());
    restore::restore(&ropts, &NoProgress).unwrap();

    assert_eq!(read_tree(&root), read_tree(&out.path().join("data")));
    assert!(out.path().join("data/emptydir").is_dir());
}

#[test]
fn multiple_files_pack_into_one_archive() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fs::write(&a, b"file a").unwrap();
    fs::write(&b, b"file b").unwrap();

    let mut opts = GrowOptions::new(vec![a, b], Target::Size(4000));
    opts.output = Some(dir.path().join("arc.fa10"));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();
    assert_eq!(outcome.entry_count, 2);
    assert_eq!(outcome.output_size, 4000);

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(dir.path().join("arc.fa10"));
    ropts.output = Some(out.path().to_path_buf());
    restore::restore(&ropts, &NoProgress).unwrap();
    assert_eq!(fs::read(out.path().join("a.txt")).unwrap(), b"file a");
    assert_eq!(fs::read(out.path().join("b.txt")).unwrap(), b"file b");
}

#[test]
fn duplicate_basename_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("x")).unwrap();
    fs::create_dir(dir.path().join("y")).unwrap();
    let a = dir.path().join("x/dup.txt");
    let b = dir.path().join("y/dup.txt");
    fs::write(&a, b"1").unwrap();
    fs::write(&b, b"2").unwrap();

    let opts = GrowOptions::new(vec![a, b], Target::Size(2000));
    assert!(grow::grow(&opts, &NoProgress).is_err());
}

#[test]
fn deterministic_archive_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("t");
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("a"), b"aaa").unwrap();
    fs::write(root.join("sub/b"), b"bbb").unwrap();

    let mk = |out: &Path| {
        let mut o = GrowOptions::new(vec![root.clone()], Target::Size(5000));
        o.output = Some(out.to_path_buf());
        grow::grow(&o, &NoProgress).unwrap();
        fs::read(out).unwrap()
    };
    assert_eq!(
        mk(&dir.path().join("1.fa10")),
        mk(&dir.path().join("2.fa10"))
    );
}

#[test]
fn explicit_size_below_minimum_errors() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, vec![0u8; 1000]).unwrap();
    let opts = GrowOptions::new(vec![original], Target::Size(10));
    assert!(grow::grow(&opts, &NoProgress).is_err());
}

#[test]
fn corrupted_content_fails_verification() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.bin");
    fs::write(&original, pseudo_random(2000)).unwrap();

    let opts = GrowOptions::new(vec![original], Target::Multiplier(4.0));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    // Flip a byte inside the content region (just after the 8-byte header).
    let mut bytes = fs::read(&outcome.output_path).unwrap();
    bytes[12] ^= 0xFF;
    fs::write(&outcome.output_path, &bytes).unwrap();

    let out = tempfile::tempdir().unwrap();
    let ropts = RestoreOptions {
        input: outcome.output_path.clone(),
        output: Some(out.path().to_path_buf()),
        verify: true,
        force: true,
    };
    assert!(restore::restore(&ropts, &NoProgress).is_err());
}

#[test]
fn custom_pattern_roundtrips_and_appears_in_output() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("note.txt");
    let data = b"the quick brown fox";
    fs::write(&original, data).unwrap();

    let mut opts = GrowOptions::new(vec![original], Target::Size(3000));
    opts.pattern = "XYZZY-".to_string();
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    let bytes = fs::read(&outcome.output_path).unwrap();
    assert!(bytes.windows(6).any(|w| w == b"XYZZY-"));

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(outcome.output_path);
    ropts.output = Some(out.path().to_path_buf());
    restore::restore(&ropts, &NoProgress).unwrap();
    assert_eq!(fs::read(out.path().join("note.txt")).unwrap(), data);
}

#[test]
fn empty_pattern_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"data").unwrap();
    let mut opts = GrowOptions::new(vec![original], Target::Size(2000));
    opts.pattern = String::new();
    assert!(grow::grow(&opts, &NoProgress).is_err());
}

#[test]
fn in_place_requires_confirm_and_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"some content here").unwrap();

    let mut opts = GrowOptions::new(vec![original.clone()], Target::Size(2000));
    opts.in_place = true;
    assert!(grow::grow(&opts, &NoProgress).is_err()); // no confirm
    assert_eq!(fs::read(&original).unwrap(), b"some content here");

    // In-place on a directory is refused even with confirm.
    let d = dir.path().join("adir");
    fs::create_dir(&d).unwrap();
    fs::write(d.join("x"), b"x").unwrap();
    let mut dopts = GrowOptions::new(vec![d], Target::Size(2000));
    dopts.in_place = true;
    dopts.confirm = true;
    assert!(grow::grow(&dopts, &NoProgress).is_err());
}

#[test]
fn in_place_with_confirm_replaces_original() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    let data = b"some content here";
    fs::write(&original, data).unwrap();

    let mut opts = GrowOptions::new(vec![original.clone()], Target::Size(2000));
    opts.in_place = true;
    opts.confirm = true;
    let outcome = grow::grow(&opts, &NoProgress).unwrap();
    assert_eq!(outcome.output_path, original);
    assert_eq!(fs::metadata(&original).unwrap().len(), 2000);
    assert!(!dir.path().join("f.txt.fa10.tmp").exists());

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(original);
    ropts.output = Some(out.path().to_path_buf());
    restore::restore(&ropts, &NoProgress).unwrap();
    assert_eq!(fs::read(out.path().join("f.txt")).unwrap(), data);
}

#[test]
fn restore_refuses_to_overwrite_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"hello there").unwrap();
    let opts = GrowOptions::new(vec![original], Target::Size(2000));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    let out = tempfile::tempdir().unwrap();
    fs::write(out.path().join("f.txt"), b"existing").unwrap();
    let mut ropts = RestoreOptions::new(outcome.output_path);
    ropts.output = Some(out.path().to_path_buf());
    assert!(restore::restore(&ropts, &NoProgress).is_err());
    // Existing file untouched.
    assert_eq!(fs::read(out.path().join("f.txt")).unwrap(), b"existing");
}

#[test]
fn restore_rejects_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("evil.fa10");

    // Hand-build an archive whose manifest references an escaping path.
    let content = b"x";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(HEADER_MAGIC);
    bytes.extend_from_slice(content);
    let manifest = Manifest {
        entries: vec![Entry {
            kind: EntryKind::File,
            path: "../escaped.txt".to_string(),
            size: content.len() as u64,
            sha256: [0u8; 32],
        }],
    };
    manifest.write_to(&mut bytes).unwrap();
    fs::write(&archive, &bytes).unwrap();

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(archive);
    ropts.output = Some(out.path().to_path_buf());
    ropts.verify = false;
    assert!(restore::restore(&ropts, &NoProgress).is_err());
    assert!(!out.path().parent().unwrap().join("escaped.txt").exists());
}

#[cfg(unix)]
#[test]
fn followed_symlink_is_stored_as_file() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("d");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("real.txt"), b"real content").unwrap();
    symlink(root.join("real.txt"), root.join("link.txt")).unwrap();

    let opts = GrowOptions::new(vec![root], Target::Multiplier(2.0));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(outcome.output_path);
    ropts.output = Some(out.path().to_path_buf());
    restore::restore(&ropts, &NoProgress).unwrap();

    // The link was followed: both paths exist as plain files with the content.
    assert_eq!(
        fs::read(out.path().join("d/real.txt")).unwrap(),
        b"real content"
    );
    assert_eq!(
        fs::read(out.path().join("d/link.txt")).unwrap(),
        b"real content"
    );
    assert!(!out
        .path()
        .join("d/link.txt")
        .symlink_metadata()
        .unwrap()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn symlink_directory_cycle_terminates() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("d");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), b"a").unwrap();
    // d/loop -> d  (a cycle)
    symlink(&root, root.join("loop")).unwrap();

    let opts = GrowOptions::new(vec![root], Target::Multiplier(2.0));
    // Must terminate (cycle detection), not hang or overflow.
    let outcome = grow::grow(&opts, &NoProgress).unwrap();
    assert!(outcome.entry_count >= 1);
}
