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
fn grow_and_restore_through_the_binary() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("hello.txt");
    fs::write(&input, b"round trip through the CLI").unwrap();

    fa10()
        .args(["grow", "--size", "2000"])
        .arg(&input)
        .assert()
        .success();

    let grown = dir.path().join("hello.txt.fa10");
    assert!(grown.exists());
    assert_eq!(fs::metadata(&grown).unwrap().len(), 2000);

    let restored = dir.path().join("hello.out");
    fa10()
        .args(["restore", "--output"])
        .arg(&restored)
        .arg(&grown)
        .assert()
        .success();

    assert_eq!(fs::read(&restored).unwrap(), b"round trip through the CLI");
}

#[test]
fn quiet_suppresses_the_banner() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("q.txt");
    fs::write(&input, b"quiet please").unwrap();

    fa10()
        .args(["--quiet", "grow", "--size", "2000"])
        .arg(&input)
        .assert()
        .success()
        .stderr(predicate::str::contains("fa10 v").not());
}

#[test]
fn cake_alias_grows_to_double() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("c.bin");
    fs::write(&input, vec![7u8; 500]).unwrap();

    fa10().arg("cake").arg(&input).assert().success();

    let grown = dir.path().join("c.bin.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 1000);
}

#[test]
fn bare_file_defaults_to_grow() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("d.bin");
    fs::write(&input, vec![3u8; 400]).unwrap();

    // No subcommand: `fa10 <file>` should grow to 2x.
    fa10().arg("-q").arg(&input).assert().success();

    let grown = dir.path().join("d.bin.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 800);
}

#[test]
fn top_level_multiplier_implies_grow() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("e.bin");
    fs::write(&input, vec![9u8; 400]).unwrap();

    // `fa10 --multiplier 3 <file>` with no subcommand.
    fa10()
        .args(["-q", "--multiplier", "3"])
        .arg(&input)
        .assert()
        .success();

    let grown = dir.path().join("e.bin.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 1200);
}

#[test]
fn slim_alias_restores() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("f.txt");
    fs::write(&input, b"slim restores the original").unwrap();

    fa10()
        .args(["-q", "grow", "--size", "2000"])
        .arg(&input)
        .assert()
        .success();
    let grown = dir.path().join("f.txt.fa10");

    let restored = dir.path().join("f.out");
    fa10()
        .args(["-q", "slim", "--output"])
        .arg(&restored)
        .arg(&grown)
        .assert()
        .success();

    assert_eq!(fs::read(&restored).unwrap(), b"slim restores the original");
}
