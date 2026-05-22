# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-05-22

First release. The `fa10` binary reports this version (`fa10 --version`).

### Added
- `grow` command that copies a file into a `.fa10` file and pads it to a target
  size, set with `--multiplier` or `--size`.
- `restore` command that recovers the original file and verifies it against the
  stored SHA-256.
- `info` command that prints a `.fa10` file's metadata without changing it.
- Themed aliases: `cake` (2x), `feast` (5x), `buffet` (10x), and `diet` / `fast`
  for restore.
- `.fa10` format with a 5-byte header, a recognizable repeating padding pattern,
  a footer holding the original size, filename, SHA-256, and a CRC32, and a
  fixed 16-byte trailer that makes the footer readable with two seeks.
- Safety checks: sibling-file output by default, a protected-path blocklist, a
  2 GiB free-space floor, a 10 GiB cap on unconfirmed output, and a 100-file
  batch limit.
- Flags: `--output`, `--pattern`, `--in-place`, `--confirm`, `--verify`,
  `--no-verify`, `--force`, `--batch`, `--quiet`, and `--verbose`.

[0.1.0]: https://github.com/walangstudio/fa10/releases/tag/v0.1.0
