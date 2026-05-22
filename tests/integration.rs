use std::fs;
use std::io::Read;

use fa10::format::{END_MAGIC, HEADER_MAGIC};
use fa10::grow::{GrowOptions, Target};
use fa10::progress::NoProgress;
use fa10::restore::RestoreOptions;
use fa10::{grow, info, restore};
use sha2::{Digest, Sha256};

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

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

#[test]
fn grow_then_restore_roundtrips_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("payload.bin");
    let data = pseudo_random(4096);
    fs::write(&original, &data).unwrap();
    let original_sha = sha256(&data);

    let opts = GrowOptions::new(original.clone(), Target::Multiplier(3.0));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    assert_eq!(outcome.output_path, dir.path().join("payload.bin.fa10"));
    assert_eq!(outcome.original_size, data.len() as u64);
    assert_eq!(outcome.sha256, original_sha);
    assert_eq!(outcome.output_size, data.len() as u64 * 3);

    // Check header and trailer magic on disk.
    let grown = fs::read(&outcome.output_path).unwrap();
    assert_eq!(grown.len() as u64, outcome.output_size);
    assert_eq!(&grown[..5], HEADER_MAGIC);
    assert_eq!(&grown[grown.len() - 16..grown.len() - 8], END_MAGIC);

    // The recognizable padding pattern should appear in the body.
    assert!(grown
        .windows(b"FA10-PADDING-BLOCK-".len())
        .any(|w| w == b"FA10-PADDING-BLOCK-"));

    // info reports sane metadata.
    let meta = info::info(&outcome.output_path).unwrap();
    assert_eq!(meta.original_size, data.len() as u64);
    assert_eq!(meta.original_filename, "payload.bin");
    assert_eq!(meta.sha256, original_sha);
    assert!((meta.multiplier - 3.0).abs() < 1e-9);

    // Restore to a fresh location and compare bytes + hash.
    let restored_path = dir.path().join("restored_payload.bin");
    let mut ropts = RestoreOptions::new(outcome.output_path.clone());
    ropts.output = Some(restored_path.clone());
    let r = restore::restore(&ropts, &NoProgress).unwrap();
    assert!(r.verified);

    let mut restored = Vec::new();
    fs::File::open(&restored_path)
        .unwrap()
        .read_to_end(&mut restored)
        .unwrap();
    assert_eq!(restored, data, "restored bytes must equal the original");
    assert_eq!(sha256(&restored), original_sha);
}

#[test]
fn grow_with_explicit_size() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"hello world, this is a small file").unwrap();

    let opts = GrowOptions::new(original.clone(), Target::Size(4000));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();
    assert_eq!(outcome.output_size, 4000);

    let restored = dir.path().join("out.txt");
    let mut ropts = RestoreOptions::new(outcome.output_path);
    ropts.output = Some(restored.clone());
    restore::restore(&ropts, &NoProgress).unwrap();
    assert_eq!(
        fs::read(&restored).unwrap(),
        b"hello world, this is a small file"
    );
}

#[test]
fn explicit_size_below_minimum_errors() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, vec![0u8; 1000]).unwrap();

    let opts = GrowOptions::new(original, Target::Size(10));
    assert!(grow::grow(&opts, &NoProgress).is_err());
}

#[test]
fn corrupted_content_fails_verification() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.bin");
    fs::write(&original, pseudo_random(2000)).unwrap();

    let opts = GrowOptions::new(original, Target::Multiplier(4.0));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    // Flip a byte inside the content region (just after the 5-byte header).
    let mut bytes = fs::read(&outcome.output_path).unwrap();
    bytes[10] ^= 0xFF;
    fs::write(&outcome.output_path, &bytes).unwrap();

    let ropts = RestoreOptions {
        input: outcome.output_path.clone(),
        output: Some(dir.path().join("nope.bin")),
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

    let mut opts = GrowOptions::new(original.clone(), Target::Size(3000));
    opts.pattern = "XYZZY-".to_string();
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    let grown = fs::read(&outcome.output_path).unwrap();
    assert!(grown.windows(6).any(|w| w == b"XYZZY-"));

    let restored = dir.path().join("note.out");
    let mut ropts = RestoreOptions::new(outcome.output_path);
    ropts.output = Some(restored.clone());
    restore::restore(&ropts, &NoProgress).unwrap();
    assert_eq!(fs::read(&restored).unwrap(), data);
}

#[test]
fn empty_pattern_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"data").unwrap();

    let mut opts = GrowOptions::new(original, Target::Size(2000));
    opts.pattern = String::new();
    assert!(grow::grow(&opts, &NoProgress).is_err());
}

#[test]
fn in_place_requires_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"some content here").unwrap();

    let mut opts = GrowOptions::new(original.clone(), Target::Size(2000));
    opts.in_place = true;
    opts.confirm = false;
    assert!(grow::grow(&opts, &NoProgress).is_err());

    // The original must be untouched after the refusal.
    assert_eq!(fs::read(&original).unwrap(), b"some content here");
}

#[test]
fn in_place_with_confirm_replaces_original() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    let data = b"some content here";
    fs::write(&original, data).unwrap();

    let mut opts = GrowOptions::new(original.clone(), Target::Size(2000));
    opts.in_place = true;
    opts.confirm = true;
    let outcome = grow::grow(&opts, &NoProgress).unwrap();
    assert_eq!(outcome.output_path, original);
    assert_eq!(fs::metadata(&original).unwrap().len(), 2000);

    // And it still restores cleanly.
    let restored = dir.path().join("back.txt");
    let mut ropts = RestoreOptions::new(original);
    ropts.output = Some(restored.clone());
    restore::restore(&ropts, &NoProgress).unwrap();
    assert_eq!(fs::read(&restored).unwrap(), data);
}

#[test]
fn restore_uses_recorded_filename_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original_name.dat");
    fs::write(&original, b"keep my name").unwrap();

    let opts = GrowOptions::new(original.clone(), Target::Size(2000));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    // Remove the original so restore can recreate it at its recorded name.
    fs::remove_file(&original).unwrap();

    let r = restore::restore(&RestoreOptions::new(outcome.output_path), &NoProgress).unwrap();
    assert_eq!(r.output_path, dir.path().join("original_name.dat"));
    assert_eq!(fs::read(r.output_path).unwrap(), b"keep my name");
}

#[test]
fn restore_refuses_to_overwrite_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("f.txt");
    fs::write(&original, b"hello there").unwrap();

    let opts = GrowOptions::new(original.clone(), Target::Size(2000));
    let outcome = grow::grow(&opts, &NoProgress).unwrap();

    // The original still exists, so restoring to its recorded name should fail.
    assert!(restore::restore(&RestoreOptions::new(outcome.output_path), &NoProgress).is_err());
}
