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

#[test]
fn diet_alias_restores() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("g.txt");
    fs::write(&input, b"diet recovers it too").unwrap();

    fa10()
        .args(["-q", "--size", "2000"])
        .arg(&input)
        .assert()
        .success();
    let grown = dir.path().join("g.txt.fa10");

    let restored = dir.path().join("g.out");
    fa10()
        .args(["-q", "diet", "--output"])
        .arg(&restored)
        .arg(&grown)
        .assert()
        .success();

    assert_eq!(fs::read(&restored).unwrap(), b"diet recovers it too");
}

#[test]
fn feast_and_buffet_aliases_scale() {
    for (alias, factor) in [("feast", 5), ("buffet", 10)] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("b.bin");
        fs::write(&input, vec![1u8; 1000]).unwrap();

        fa10().arg("-q").arg(alias).arg(&input).assert().success();

        let grown = dir.path().join("b.bin.fa10");
        assert_eq!(
            fs::metadata(&grown).unwrap().len(),
            1000 * factor,
            "alias {alias}"
        );
    }
}

#[test]
fn implicit_grow_with_size_hits_target() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("s.bin");
    fs::write(&input, vec![2u8; 1000]).unwrap();

    // No subcommand, absolute size.
    fa10()
        .args(["-q", "--size", "5000"])
        .arg(&input)
        .assert()
        .success();

    let grown = dir.path().join("s.bin.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 5000);
}

#[test]
fn implicit_grow_custom_pattern_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("p.txt");
    fs::write(&input, b"pattern round trip").unwrap();

    // `--pattern` before the file: the implicit grow must still apply, and the
    // value must not be mistaken for a subcommand.
    fa10()
        .args(["-q", "--pattern", "ZZZ-", "--size", "4000"])
        .arg(&input)
        .assert()
        .success();
    let grown = dir.path().join("p.txt.fa10");
    assert_eq!(fs::metadata(&grown).unwrap().len(), 4000);

    let restored = dir.path().join("p.out");
    fa10()
        .args(["-q", "restore", "--output"])
        .arg(&restored)
        .arg(&grown)
        .assert()
        .success();
    assert_eq!(fs::read(&restored).unwrap(), b"pattern round trip");
}

#[test]
fn file_named_like_a_subcommand_needs_explicit_grow() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("cake");
    fs::write(&input, vec![4u8; 600]).unwrap();

    // `fa10 grow cake` grows the file literally named "cake".
    fa10().args(["-q", "grow"]).arg(&input).assert().success();
    assert_eq!(
        fs::metadata(dir.path().join("cake.fa10")).unwrap().len(),
        1200
    );
}

#[test]
fn info_reports_multiplier_and_matches_verbose_sha() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("h.bin");
    fs::write(&input, vec![5u8; 1000]).unwrap();

    // Capture the SHA printed by a verbose grow.
    let out = fa10()
        .args(["-v", "--size", "2000"])
        .arg(&input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(out).unwrap();
    let sha_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("sha256:"))
        .expect("verbose output should print sha256");
    let grow_sha = sha_line.split_whitespace().last().unwrap().to_string();

    let grown = dir.path().join("h.bin.fa10");
    fa10()
        .args(["-q", "info"])
        .arg(&grown)
        .assert()
        .success()
        .stdout(predicate::str::contains("multiplier:        2.00x"))
        .stdout(predicate::str::contains(&grow_sha));
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
    let grown = dir.path().join("x.bin.fa10");
    fs::write(&grown, b"preexisting").unwrap();

    fa10().arg("-q").arg(&input).assert().failure();

    // The pre-existing file must be left exactly as it was, and no temp file
    // should be created.
    assert_eq!(fs::read(&grown).unwrap(), b"preexisting");
    assert!(!dir.path().join("x.bin.fa10.fa10.tmp").exists());
}

#[test]
fn in_place_grow_leaves_no_temp_and_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("ip.txt");
    fs::write(&input, b"in place round trip").unwrap();

    // In-place grow needs --confirm; on success the .tmp is renamed away.
    fa10()
        .args(["-q", "grow", "--in-place", "--confirm", "--size", "3000"])
        .arg(&input)
        .assert()
        .success();

    assert_eq!(fs::metadata(&input).unwrap().len(), 3000);
    assert!(!dir.path().join("ip.txt.fa10.tmp").exists());

    let restored = dir.path().join("ip.out");
    fa10()
        .args(["-q", "restore", "--output"])
        .arg(&restored)
        .arg(&input)
        .assert()
        .success();
    assert_eq!(fs::read(&restored).unwrap(), b"in place round trip");
}
