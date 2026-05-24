# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-05-24

First release. fa10 is the opposite of zip: it packs files and directories into
one larger, fully-reversible `.fa10` archive and extracts the whole tree back
byte-for-byte. The `fa10` binary reports this version (`fa10 --version`).

### Added
- `inflate` packs files and/or directories into one archive, padding to a target
  size set with `--multiplier` (a multiple of the total input size) or `--size`.
  Directories are walked recursively; symlinks are followed (their content is
  stored as a regular file) with cycle detection; empty directories are kept.
  `inflate` is the default action, so `fa10 <path>` and `fa10 --multiplier 5 <path>`
  work without naming it (`grow` remains a hidden alias).
- `restore` extracts the stored tree under the current directory (or
  `--output <dir>`), like `unzip`, verifying each entry against its SHA-256 and
  refusing to overwrite without `--force`.
- `info` lists every entry and the archive metadata; `-v` adds per-entry SHA-256.
- Themed aliases: `cake` (2x), `feast` (5x), `buffet` (10x), and `diet` / `slim`
  for restore.
- `.fa10` archive format: 8-byte header, concatenated entry contents,
  recognizable repeating padding, then a manifest (per-entry kind, path, size,
  and SHA-256, with a CRC32) and a fixed 16-byte trailer. Entries are sorted by
  path, so the same input tree always produces a byte-identical archive.
- Safety checks: sibling-file output by default, a protected-path blocklist, a
  2 GiB free-space floor on pack and extract, a 10 GiB cap on unconfirmed
  output, a 100-file batch limit, and a Zip-Slip guard that refuses absolute,
  drive-qualified, or `..` archive paths on extraction. The manifest parser
  rejects malformed input (oversized lengths/counts) without panicking.
- Optional `progress` feature (on by default) for the `indicatif` progress bar;
  `--no-default-features` builds a smaller binary without it.
- Flags: `--output`, `--pattern`, `--in-place`, `--confirm`, `--verify`,
  `--no-verify`, `--force`, `--batch`, `--quiet`, and `--verbose`.
- Prebuilt binaries (Linux/macOS x86_64 + arm64, Windows x86_64) with
  `SHA256SUMS`, built and published by the release workflow on each `v*` tag.

[0.2.0]: https://github.com/walangstudio/fa10/releases/tag/v0.2.0
