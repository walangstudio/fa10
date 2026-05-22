# fa10

> Grow a file into a larger, **fully-reversible** test file with recognizable padding.

`fa10` takes an input file and produces a bigger `.fa10` file by appending a
repeating, human-recognizable ASCII pattern plus a small metadata footer. The
original can be restored **byte-for-byte**, verified with SHA-256.

It's useful for:

- generating large files for **storage benchmarks**,
- exercising **backup tools** and **upload limits**,
- and being a fun little CLI in the spirit of `cowsay` or `sl`.

Unlike filling a file with random bytes, `fa10`'s padding is obvious in a hex
dump (`FA10-PADDING-BLOCK-FA10-PADDING-BLOCK-…`), compresses trivially, and is
trivially reversible.

```text
$ fa10 grow --multiplier 5 report.csv
grew report.csv -> report.csv.fa10 (1.20 MiB -> 6.00 MiB, 4.80 MiB padding)

$ fa10 info report.csv.fa10
original filename: report.csv
original size:     1.20 MiB
total size:        6.00 MiB
multiplier:        5.00x
original sha256:   79279faed3a1...

$ fa10 restore report.csv.fa10
restored report.csv.fa10 -> report.csv (1.20 MiB), SHA-256 verified
```

## Install

### From source (cargo)

```sh
cargo install --path .
# or, once published:
cargo install fa10
```

### Homebrew

```sh
brew install walangstudio/tap/fa10   # tap publishing tracked separately
```

### GitHub Releases

Prebuilt binaries for Linux, macOS, and Windows (x86_64 + arm64) are attached to
each [release](https://github.com/walangstudio/fa10/releases). Binaries are
shipped **unpacked** (no UPX). macOS builds are notarized and Windows builds are
Authenticode-signed (see the release workflow).

## Usage

```text
fa10 grow <file>...                 # grow by 2x (default)
fa10 grow --multiplier 5 <file>     # grow to 5x the original size
fa10 grow --size 100MB <file>       # grow to an absolute size (binary units)
fa10 restore <file.fa10>...         # restore the original
fa10 info <file.fa10>               # inspect metadata, no restore
```

### Themed aliases (sugar over `--multiplier`)

```text
fa10 cake   <file>        # 2x
fa10 feast  <file>        # 5x
fa10 buffet <file>        # 10x
fa10 diet   <file.fa10>   # alias for restore
fa10 fast   <file.fa10>   # alias for restore
```

### Flags

| Flag | Applies to | Meaning |
|------|------------|---------|
| `-m`, `--multiplier <N>` | grow | Output size as a multiple of the original (default `2`). |
| `-s`, `--size <SIZE>` | grow | Absolute target size, e.g. `100MB`, `2GiB`. Conflicts with `--multiplier`. |
| `-o`, `--output <PATH>` | grow / restore | Explicit output path (single file only). |
| `--pattern <STR>` | grow | Custom padding pattern (default `FA10-PADDING-BLOCK-`). |
| `--in-place` | grow | Replace the original (requires `--confirm`). |
| `--confirm` | grow | Authorize in-place writes and outputs above the 10 GiB cap. |
| `--verify` | grow | Re-read and SHA-256-verify the written file. |
| `--no-verify` | restore | Skip SHA-256 verification of recovered content. |
| `--force` | restore | Overwrite an existing output file. |
| `--batch` | grow / restore | Allow operating on more than 100 input files. |
| `-q`, `--quiet` | global | Suppress banner and progress bar. |
| `-v`, `--verbose` | global | Print extra detail (hashes). |

### Size units

All units are **binary** (1024-based). `KB`/`MB`/`GB`/`TB` are treated the same
as `KiB`/`MiB`/`GiB`/`TiB`. A bare number is bytes. Decimals are allowed
(`1.5MB` → `1572864`).

## File format (`.fa10`)

```text
Offset            Size   Field
------            ----   -----
0                 5      Header magic        "FA10\x00"
5                 N      Original content    (N = original_size bytes, verbatim)
5 + N             P      Padding             repeating ASCII "FA10-PADDING-BLOCK-"
                                              (or --pattern), truncated to fill P bytes
--- Footer (length F = 56 + L) -------------------------------------------------
5+N+P             8      Footer magic        "FA10FOOT"
+8                8      original_size        u64 LE
+8                4      filename_len  (L)    u32 LE
+4                L      original_filename    UTF-8
+L                32     SHA-256 of original content
+32               4      CRC32 of footer bytes [footer_start .. this field)
--- Trailer (fixed 16 bytes, at very end) --------------------------------------
EOF-16            8      End magic           "FA10END\x00"
EOF-8             8      footer_length (F)    u64 LE
```

- **Padding** `P = total_size - 5 - N - F - 16`.
- The fixed 16-byte trailer makes the variable-length footer **reverse-readable
  in O(1)**: `restore` and `info` seek straight to the footer instead of
  scanning a multi-gigabyte file.
- **Restore** reads the footer, streams the content region `[5, 5+N)` to the
  output, and verifies the SHA-256.

The `.fa10` extension was chosen because it is unclaimed (`.fa` is taken by the
FASTA bioinformatics format).

## Safety

`fa10` is a good citizen. It will, by default:

1. Write to a **sibling** file (`original.txt.fa10`) and never touch the original
   unless `--in-place --confirm` is given.
2. Refuse to operate on **protected system paths** (`/usr`, `/bin`, `/etc`,
   `/System`, `~/Library`, `C:\Windows`, …).
3. Refuse to write if it would leave **less than 2 GiB** free.
4. Cap unconfirmed output at **10 GiB**; larger requires `--confirm`.
5. Refuse batches of **more than 100 files** without `--batch`.
6. Do **no** network, registry, autostart, or self-modification — it is a pure
   local-filesystem tool.

See [SECURITY.md](SECURITY.md) for details.

## Development

```sh
cargo test                       # unit + integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## License

[MIT](LICENSE) © fa10 contributors.
