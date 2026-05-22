# fa10

[![CI](https://github.com/walangstudio/fa10/actions/workflows/ci.yml/badge.svg)](https://github.com/walangstudio/fa10/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/fa10.svg)](https://crates.io/crates/fa10)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.74+](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org)

fa10 makes a file bigger and lets you get the original back exactly.

It copies your file into a new `.fa10` file, pads it out to whatever size you
ask for using a repeating text marker, and records enough metadata in a footer
to undo the whole thing. The padding is plain ASCII (`FA10-PADDING-BLOCK-`
over and over), so it shows up clearly in a hex dump and compresses to almost
nothing, unlike random bytes.

I wrote it because I kept needing large files to test backups, upload limits,
and disk-space behaviour, and `dd if=/dev/urandom` left me with junk I couldn't
turn back into the original. fa10 keeps the original recoverable and verifies it
with SHA-256 on the way back.

```
$ fa10 grow --multiplier 5 report.csv
grew report.csv -> report.csv.fa10 (1.20 MiB -> 6.00 MiB, 4.80 MiB padding)

$ fa10 restore report.csv.fa10
restored report.csv.fa10 -> report.csv (1.20 MiB), SHA-256 verified
```

## Install

From source:

```sh
cargo install --path .
```

Once it is on crates.io:

```sh
cargo install fa10
```

Prebuilt binaries for Linux, macOS, and Windows (x86_64 and arm64) are attached
to each [release](https://github.com/walangstudio/fa10/releases). They ship
unpacked. macOS builds are notarized and Windows builds are Authenticode-signed.

## Usage

```
fa10 grow <file>...                 grow by 2x (the default)
fa10 grow --multiplier 5 <file>     grow to 5x the original size
fa10 grow --size 100MB <file>       grow to a fixed size
fa10 restore <file.fa10>...         get the original back
fa10 info <file.fa10>               print metadata, change nothing
```

There are themed aliases if you want them:

```
fa10 cake   <file>        same as grow --multiplier 2
fa10 feast  <file>        same as grow --multiplier 5
fa10 buffet <file>        same as grow --multiplier 10
fa10 diet   <file.fa10>   same as restore
fa10 fast   <file.fa10>   same as restore
```

### Flags

| Flag | Command | What it does |
|------|---------|--------------|
| `-m`, `--multiplier <N>` | grow | Output size as a multiple of the original. Default is 2. |
| `-s`, `--size <SIZE>` | grow | Fixed target size, for example `100MB` or `2GiB`. Cannot be combined with `--multiplier`. |
| `-o`, `--output <PATH>` | grow, restore | Where to write the result. Single file only. |
| `--pattern <STR>` | grow | Padding text to repeat. Default is `FA10-PADDING-BLOCK-`. |
| `--in-place` | grow | Replace the original. Requires `--confirm`. |
| `--confirm` | grow | Allow in-place writes and output over the 10 GiB cap. |
| `--verify` | grow | Re-read the result and check its SHA-256 before reporting success. |
| `--no-verify` | restore | Skip the SHA-256 check on the recovered file. |
| `--force` | restore | Overwrite the output file if it already exists. |
| `--batch` | grow, restore | Allow more than 100 input files in one run. |
| `-q`, `--quiet` | any | No banner, no progress bar. |
| `-v`, `--verbose` | any | Print hashes as well. |

### Sizes

Sizes use 1024 as the base. `KB`, `MB`, `GB`, and `TB` mean the same thing as
`KiB`, `MiB`, `GiB`, and `TiB`. A plain number is a byte count. Decimals work,
so `1.5MB` is `1572864` bytes.

## File format

```
Offset            Size   Field
0                 5      header magic        "FA10\x00"
5                 N      original content    (N = original size, copied as-is)
5 + N             P      padding             repeating "FA10-PADDING-BLOCK-" (or --pattern)
5 + N + P         8      footer magic        "FA10FOOT"
+8                8      original_size        u64 little-endian
+8                4      filename length (L)  u32 little-endian
+4                L      original filename    UTF-8
+L                32     SHA-256 of original content
+32               4      CRC32 of the footer bytes up to here
EOF - 16          8      end magic           "FA10END\x00"
EOF - 8           8      footer length        u64 little-endian
```

The 16-byte trailer at the end holds the footer length, so restore and info can
jump straight to the footer with two seeks instead of scanning the whole file.
Restore reads the footer, copies the content region back out, and checks the
SHA-256.

The `.fa10` extension is used because nothing else claims it. `.fa` is taken by
the FASTA format from bioinformatics.

## Safety

fa10 tries not to surprise you:

1. It writes to a sibling file (`name.ext.fa10`) and leaves the original alone
   unless you pass both `--in-place` and `--confirm`.
2. It refuses to touch system paths such as `/usr`, `/bin`, `/etc`, `/System`,
   `~/Library`, and `C:\Windows`.
3. It checks free space first and refuses if the write would leave under 2 GiB.
4. Output above 10 GiB needs `--confirm`.
5. More than 100 files in one run needs `--batch`.
6. It does not use the network, write config or registry entries, set up
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

## License

MIT. See [LICENSE](LICENSE).
