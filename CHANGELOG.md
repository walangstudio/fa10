# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-05-23

fa10 is now an archiver: the opposite of zip. It packs files **and directories**
into one larger, fully-reversible `.fa10` archive and extracts the whole tree
back byte-for-byte. This replaces the single-file format; there is no 0.1.0
release in the wild, so no migration is needed.

### Added
- Pack directories and multiple inputs into one archive. A directory is walked
  recursively; symlinks are followed (their content is stored as a regular
  file) with cycle detection. Empty directories are preserved.
- Multi-entry archive format: header `FA10ARC\0`, concatenated content,
  recognizable padding, then a manifest (per-entry kind/path/size/SHA-256 +
  CRC32) and a 16-byte trailer. Entries are sorted by path, so the same tree
  always produces byte-identical output.
- `restore` extracts the stored tree under the current directory (or
  `--output <dir>`), like `unzip`, refusing to overwrite without `--force`.
- `info` lists every entry; `-v` adds per-entry SHA-256.
- Zip-Slip guard: archive paths that are absolute, drive-qualified, or contain
  `..` are refused on extraction.
- Multiplier now scales the total input size; output naming is `<input>.fa10`,
  `<dir>.fa10`, or `archive.fa10` for 2+ loose files.

### Changed
- `--in-place` is restricted to a single file input.
- `grow` no longer writes one `.fa10` per input; all inputs go into one archive.

## [0.1.0] - 2026-05-22

First release. The `fa10` binary reports this version (`fa10 --version`).

### Added
- `grow` is the default command, so `fa10 <file>` and `fa10 --multiplier 5 <file>`
  work without naming `grow`. The explicit `fa10 grow <file>` still works.
- `restore` command that recovers the original file and verifies it against the
  stored SHA-256.
- `info` command that prints a `.fa10` file's metadata without changing it.
- Themed aliases: `cake` (2x), `feast` (5x), `buffet` (10x), and `diet` / `slim`
  for restore.
- Optional `progress` feature (on by default) for the `indicatif` progress bar;
  `--no-default-features` builds a smaller binary without it.
- `.fa10` format with a 5-byte header, a recognizable repeating padding pattern,
  a footer holding the original size, filename, SHA-256, and a CRC32, and a
  fixed 16-byte trailer that makes the footer readable with two seeks.
- Safety checks: sibling-file output by default, a protected-path blocklist, a
  2 GiB free-space floor, a 10 GiB cap on unconfirmed output, and a 100-file
  batch limit.
- Flags: `--output`, `--pattern`, `--in-place`, `--confirm`, `--verify`,
  `--no-verify`, `--force`, `--batch`, `--quiet`, and `--verbose`.
- Prebuilt binaries (Linux/macOS x86_64 + arm64, Windows x86_64) with
  `SHA256SUMS`, built and published by the release workflow on each `v*` tag.

[0.2.0]: https://github.com/walangstudio/fa10/releases/tag/v0.2.0
[0.1.0]: https://github.com/walangstudio/fa10/releases/tag/v0.1.0
