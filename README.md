# fa10

[![CI](https://github.com/walangstudio/fa10/actions/workflows/ci.yml/badge.svg)](https://github.com/walangstudio/fa10/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/fa10.svg)](https://crates.io/crates/fa10)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.74+](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org)

fa10 is the opposite of zip: it packs files and directories into one **bigger**,
fully-reversible `.fa10` archive, then extracts the tree back byte-for-byte.

It concatenates everything into a single archive, pads it out to whatever size
you ask for using a repeating text marker, and records a manifest (each entry's
path, size, and SHA-256) so the whole tree can be rebuilt. The padding is plain
ASCII (`FA10-PADDING-BLOCK-` over and over), so it shows up clearly in a hex dump
and compresses to almost nothing, unlike random bytes.

I wrote it because I kept needing large files and trees to test backups, upload
limits, and disk-space behaviour, and `dd if=/dev/urandom` left me with junk I
couldn't turn back into the originals. fa10 keeps everything recoverable and
verifies each entry with SHA-256 on the way back.

```
$ fa10 --multiplier 5 project/
packed 42 entries (1.20 MiB) -> project.fa10 (6.00 MiB, 4.80 MiB padding)

$ fa10 restore project.fa10
extracted 42 entries from project.fa10 -> . (1.20 MiB), SHA-256 verified
```

## Install

### Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/walangstudio/fa10/main/install.sh | sh
```

This downloads the right prebuilt binary, verifies its SHA-256, and installs it
to `/usr/local/bin` (or `~/.local/bin` if that is not writable). Re-run it any
time to upgrade. Options: `--version v0.1.0` for a specific release,
`--pre-release` for the latest pre-release.

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/walangstudio/fa10/main/install.ps1 | iex
```

Installs to `%LOCALAPPDATA%\Programs\fa10` and adds it to your user PATH. For
options, run the script explicitly:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/walangstudio/fa10/main/install.ps1))) -Version v0.1.0
```

### With cargo

```sh
cargo install fa10                     # from crates.io, once published
cargo install --git https://github.com/walangstudio/fa10   # from source
```

`cargo binstall fa10` also works if you have [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) (it pulls the prebuilt binary).

### Manual

Prebuilt binaries are attached to each
[release](https://github.com/walangstudio/fa10/releases): Linux and macOS on
x86_64 and arm64, Windows on x86_64, shipped as `.tar.gz` (Unix) / `.zip`
(Windows) with a `SHA256SUMS` file. Download, verify, extract the single `fa10`
binary, and put it anywhere on your `PATH`.

## Uninstall

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/walangstudio/fa10/main/install.sh | sh -s -- --uninstall
```

```powershell
# Windows
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/walangstudio/fa10/main/install.ps1))) -Uninstall
```

Or just delete the `fa10` binary from wherever it was installed. fa10 keeps no
config, cache, or registry state, so removing the binary removes everything.

## Usage

`grow` is the default, so a bare path just packs it:

```
fa10 <path>...                      pack by 2x total size (the default)
fa10 --multiplier 5 <path>...       pack to 5x the total input size
fa10 --size 100MB <path>...         pack to a fixed size
fa10 mydir/                         pack a directory tree -> mydir.fa10
fa10 a.txt b.txt -o out.fa10        pack several files into one archive
fa10 restore <archive>...           extract the tree (into the current dir)
fa10 info <archive>                 list entries and metadata, change nothing
```

Output naming: one file `foo` becomes `foo.fa10`; one directory `bar` becomes
`bar.fa10`; two or more loose files default to `archive.fa10` (or pass
`--output`). Extraction recreates the stored tree under the current directory,
or under `--output <dir>`, like `unzip`.

`fa10 grow <path>` still works if you prefer to spell it out. The implicit
`grow` only kicks in when the first argument is not a known subcommand, so a
file literally named `restore` needs `fa10 grow restore`.

There are themed aliases if you want them:

```
fa10 cake   <path>...     same as grow --multiplier 2
fa10 feast  <path>...     same as grow --multiplier 5
fa10 buffet <path>...     same as grow --multiplier 10
fa10 diet   <archive>     same as restore
fa10 slim   <archive>     same as restore
```

### Flags

| Flag | Command | What it does |
|------|---------|--------------|
| `-m`, `--multiplier <N>` | grow | Output size as a multiple of the total input size. Default is 2. |
| `-s`, `--size <SIZE>` | grow | Fixed target size, for example `100MB` or `2GiB`. Cannot be combined with `--multiplier`. |
| `-o`, `--output <PATH>` | grow | Archive path. Defaults to `<input>.fa10`, or `archive.fa10` for 2+ inputs. |
| `-o`, `--output <DIR>` | restore | Directory to extract into. Defaults to the current directory. |
| `--pattern <STR>` | grow | Padding text to repeat. Default is `FA10-PADDING-BLOCK-`. |
| `--in-place` | grow | Replace a single input file with its archive. Requires `--confirm`. |
| `--confirm` | grow | Allow in-place writes and output over the 10 GiB cap. |
| `--verify` | grow | Re-read the archive and check every entry's SHA-256 before reporting success. |
| `--no-verify` | restore | Skip the SHA-256 check while extracting. |
| `--force` | restore | Overwrite existing files when extracting. |
| `--batch` | grow | Allow packing more than 100 files. |
| `-q`, `--quiet` | any | No banner, no progress bar. |
| `-v`, `--verbose` | any | With `info`, also print each entry's SHA-256. |

### Sizes

Sizes use 1024 as the base. `KB`, `MB`, `GB`, and `TB` mean the same thing as
`KiB`, `MiB`, `GiB`, and `TiB`. A plain number is a byte count. Decimals work,
so `1.5MB` is `1572864` bytes.

## File format

```
Offset    Size   Field
0         8      header magic       "FA10ARC\0"
8         ..     entry contents     concatenated in manifest order
..        P      padding            repeating "FA10-PADDING-BLOCK-" (or --pattern)
..        M      manifest:
                   magic            "FA10MANI"
                   entry_count      u32 little-endian
                   per entry:       kind u8 (0=file, 1=empty dir)
                                    path length u32 LE, path (UTF-8, '/'-separated)
                                    content size u64 LE
                                    SHA-256 of content (32 bytes)
                   crc32            u32 LE over the manifest bytes
EOF - 16  8      end magic          "FA10AEND"
EOF - 8   8      manifest length    u64 little-endian
```

Entries are sorted by path, so the same input tree always produces a
byte-identical archive. The 16-byte trailer holds the manifest length, so
restore and info reach the manifest with two seeks instead of scanning. Restore
reads the manifest, then streams the contiguous content region back out entry by
entry, checking each SHA-256.

The `.fa10` extension is used because nothing else claims it. `.fa` is taken by
the FASTA format from bioinformatics.

## Safety

fa10 tries not to surprise you:

1. It writes to a sibling file (`name.fa10` / `dir.fa10`) and leaves the inputs
   alone unless you pass both `--in-place` and `--confirm` (single file only).
2. It refuses to touch system paths such as `/usr`, `/bin`, `/etc`, `/System`,
   `~/Library`, and `C:\Windows`.
3. It checks free space first and refuses if the write would leave under 2 GiB.
4. Output above 10 GiB needs `--confirm`; packing more than 100 files needs `--batch`.
5. Extraction is guarded against Zip-Slip: entry paths that are absolute,
   drive-qualified, or contain `..` are refused, so an archive can never write
   outside the extraction directory. Existing files are not overwritten without
   `--force`.
6. Symlinks are followed at pack time (their target content is stored as a plain
   file), with cycle detection; the archive never contains a symlink, so
   extraction only ever creates regular files and directories.
7. It does not use the network, write config or registry entries, set up
   autostart, or modify itself.

See [SECURITY.md](SECURITY.md) for the details.

## Building and testing

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The test suite uses small files (a few kilobytes) so it runs in well under a
second. There is nothing that writes a large file as part of the tests.

The progress bar is behind a default `progress` feature. Build with
`cargo build --release --no-default-features` to drop the `indicatif`
dependency for a smaller binary; operations still run, just without the bar.

The toolchain is pinned in `rust-toolchain.toml`; CI uses the same version.

## Releases

Releases are built by `.github/workflows/release.yml`. Either push a tag:

```sh
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

or run the `Release` workflow manually (Actions tab) with a tag input. The
workflow stamps the crate version from the tag, builds the five target
binaries, generates `SHA256SUMS`, and publishes a GitHub Release with
auto-generated notes.

## License

MIT. See [LICENSE](LICENSE).
