use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

fn fa10() -> Command {
    Command::cargo_bin("fa10").unwrap()
}

#[test]
fn version_matches_crate() {
    fa10()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn pack_and_extract_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("hello.txt");
    fs::write(&input, b"round trip through the CLI").unwrap();

    fa10()
        .args(["-q", "inflate", "--size", "2000"])
        .arg(&input)
        .assert()
        .success();

    let grown = dir.path().join("hello.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 2000);

    let out = tempfile::tempdir().unwrap();
    fa10()
        .args(["-q", "restore", "--output"])
        .arg(out.path())
        .arg(&grown)
        .assert()
        .success();
    assert_eq!(
        fs::read(out.path().join("hello.txt")).unwrap(),
        b"round trip through the CLI"
    );
}

#[test]
fn no_banner_on_normal_run() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("q.txt");
    fs::write(&input, b"no banner please").unwrap();

    fa10()
        .args(["inflate", "--size", "2000"])
        .arg(&input)
        .assert()
        .success()
        .stderr(predicate::str::contains("fully-reversible").not());
}

#[test]
fn cake_alias_grows_to_double() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("c.bin");
    fs::write(&input, vec![7u8; 500]).unwrap();

    fa10().arg("-q").arg("cake").arg(&input).assert().success();

    let grown = dir.path().join("c.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 1000);
}

#[test]
fn bare_file_defaults_to_grow() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("d.bin");
    fs::write(&input, vec![3u8; 400]).unwrap();

    fa10().arg("-q").arg(&input).assert().success();

    assert_eq!(fs::metadata(dir.path().join("d.fa10")).unwrap().len(), 800);
}

#[test]
fn top_level_multiplier_implies_grow() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("e.bin");
    fs::write(&input, vec![9u8; 400]).unwrap();

    fa10()
        .args(["-q", "--multiplier", "3"])
        .arg(&input)
        .assert()
        .success();

    assert_eq!(fs::metadata(dir.path().join("e.fa10")).unwrap().len(), 1200);
}

#[test]
fn slim_and_diet_aliases_extract() {
    for alias in ["slim", "diet"] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("f.txt");
        fs::write(&input, b"alias extracts").unwrap();
        fa10()
            .args(["-q", "--size", "2000"])
            .arg(&input)
            .assert()
            .success();

        let out = tempfile::tempdir().unwrap();
        fa10()
            .args(["-q", alias, "--output"])
            .arg(out.path())
            .arg(dir.path().join("f.fa10"))
            .assert()
            .success();
        assert_eq!(
            fs::read(out.path().join("f.txt")).unwrap(),
            b"alias extracts"
        );
    }
}

#[test]
fn feast_and_buffet_aliases_scale() {
    for (alias, factor) in [("feast", 5), ("buffet", 10)] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("b.bin");
        fs::write(&input, vec![1u8; 1000]).unwrap();

        fa10().arg("-q").arg(alias).arg(&input).assert().success();

        assert_eq!(
            fs::metadata(dir.path().join("b.fa10")).unwrap().len(),
            1000 * factor,
            "alias {alias}"
        );
    }
}

#[test]
fn directory_packs_and_extracts() {
    let src = tempfile::tempdir().unwrap();
    let root = src.path().join("proj");
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("top.txt"), b"top").unwrap();
    fs::write(root.join("sub/nested.txt"), b"nested").unwrap();

    fa10().arg("-q").arg(&root).assert().success();
    let archive = src.path().join("proj.fa10");
    assert!(archive.exists());

    let out = tempfile::tempdir().unwrap();
    fa10()
        .args(["-q", "restore", "--output"])
        .arg(out.path())
        .arg(&archive)
        .assert()
        .success();
    assert_eq!(fs::read(out.path().join("proj/top.txt")).unwrap(), b"top");
    assert_eq!(
        fs::read(out.path().join("proj/sub/nested.txt")).unwrap(),
        b"nested"
    );
}

#[test]
fn multiple_loose_files_make_one_archive() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fs::write(&a, b"aaa").unwrap();
    fs::write(&b, b"bbb").unwrap();
    let archive = dir.path().join("arc.fa10");

    fa10()
        .args(["-q", "--size", "4000", "--output"])
        .arg(&archive)
        .arg(&a)
        .arg(&b)
        .assert()
        .success();
    assert_eq!(fs::metadata(&archive).unwrap().len(), 4000);

    fa10()
        .args(["-q", "info"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("entries:      2"))
        .stdout(predicate::str::contains("a.txt"))
        .stdout(predicate::str::contains("b.txt"));
}

#[test]
fn implicit_grow_custom_pattern_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("p.txt");
    fs::write(&input, b"pattern round trip").unwrap();

    // `--pattern` before the file: implicit grow applies and the value is not
    // mistaken for a subcommand.
    fa10()
        .args(["-q", "--pattern", "ZZZ-", "--size", "4000"])
        .arg(&input)
        .assert()
        .success();
    let grown = dir.path().join("p.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 4000);

    let out = tempfile::tempdir().unwrap();
    fa10()
        .args(["-q", "restore", "--output"])
        .arg(out.path())
        .arg(&grown)
        .assert()
        .success();
    assert_eq!(
        fs::read(out.path().join("p.txt")).unwrap(),
        b"pattern round trip"
    );
}

#[test]
fn file_named_like_a_subcommand_needs_explicit_inflate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("cake");
    fs::write(&input, vec![4u8; 600]).unwrap();

    // `fa10 inflate cake` packs the file literally named "cake".
    fa10()
        .args(["-q", "inflate"])
        .arg(&input)
        .assert()
        .success();
    assert_eq!(
        fs::metadata(dir.path().join("cake.fa10")).unwrap().len(),
        1200
    );
}

#[test]
fn grow_is_a_working_alias_of_inflate() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("g.bin");
    fs::write(&input, vec![8u8; 500]).unwrap();

    fa10()
        .args(["-q", "grow", "--size", "2000"])
        .arg(&input)
        .assert()
        .success();
    assert_eq!(fs::metadata(dir.path().join("g.fa10")).unwrap().len(), 2000);
}

#[test]
fn info_lists_entries_and_multiplier() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("h.bin");
    fs::write(&input, vec![5u8; 1000]).unwrap();
    fa10()
        .args(["-q", "--size", "2000"])
        .arg(&input)
        .assert()
        .success();

    fa10()
        .args(["-q", "info"])
        .arg(dir.path().join("h.fa10"))
        .assert()
        .success()
        .stdout(predicate::str::contains("entries:      1"))
        .stdout(predicate::str::contains("multiplier:   2.00x"))
        .stdout(predicate::str::contains("h.bin"));
}

#[test]
fn help_and_version_succeed() {
    fa10()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
    fa10()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_shows_help_and_fails() {
    fa10().assert().failure();
}

#[test]
fn failed_grow_leaves_no_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("x.bin");
    fs::write(&input, vec![6u8; 500]).unwrap();

    // Pre-create the output so grow refuses (OutputExists) before writing.
    let grown = dir.path().join("x.fa10");
    fs::write(&grown, b"preexisting").unwrap();

    fa10().arg("-q").arg(&input).assert().failure();

    assert_eq!(fs::read(&grown).unwrap(), b"preexisting");
    assert!(!dir.path().join("x.fa10.fa10.tmp").exists());
}

#[test]
fn in_place_grow_leaves_no_temp_and_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("ip.txt");
    fs::write(&input, b"in place round trip").unwrap();

    fa10()
        .args(["-q", "inflate", "--in-place", "--confirm", "--size", "3000"])
        .arg(&input)
        .assert()
        .success();

    assert_eq!(fs::metadata(&input).unwrap().len(), 3000);
    assert!(!dir.path().join("ip.txt.fa10.tmp").exists());

    let out = tempfile::tempdir().unwrap();
    fa10()
        .args(["-q", "restore", "--output"])
        .arg(out.path())
        .arg(&input)
        .assert()
        .success();
    assert_eq!(
        fs::read(out.path().join("ip.txt")).unwrap(),
        b"in place round trip"
    );
}
