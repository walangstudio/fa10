# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately via GitHub Security Advisories
("Report a vulnerability" on the repository's **Security** tab) rather than a
public issue. We aim to acknowledge reports within a few days.

## Threat model and design intent

`fa10` is a **local-filesystem CLI**. It reads an input file and writes an
output file on the same machine. It is not a sandbox and makes no attempt to
defend against an attacker who already controls the machine or the files it is
pointed at. Its safety features exist to prevent **accidental footguns** — a
mistyped path, an unintended overwrite, or a runaway size — not to contain
hostile input.

What `fa10` deliberately does **not** do:

- **No network.** It opens no sockets and contacts no servers. The crate has no
  HTTP/TLS dependencies.
- **No persistence beyond the output file.** It writes no config, cache,
  registry keys, autostart entries, or hidden state.
- **No self-modification or code execution.** It never rewrites its own binary,
  loads plugins, or executes external commands.

## Safety properties

These guardrails are enforced before any bytes are written:

1. **Original is never modified by default.** Output goes to a sibling file
   (`original.ext.fa10`). The input is only overwritten when **both**
   `--in-place` and `--confirm` are supplied, and even then the new file is
   written to a temporary path and atomically renamed into place.

2. **Protected-path blocklist.** `fa10` refuses to read from or write to known
   system locations, matched after canonicalizing the path (so symlinks and
   `..` cannot bypass it). The blocklist includes, among others:
   `/System`, `/usr`, `/bin`, `/sbin`, `/boot`, `/etc`, `/dev`, `/proc`,
   `/sys`, `/lib`, `/Library`, `~/Library`, `C:\Windows`, and
   `C:\Program Files`.

3. **Free-space floor.** Before writing, `fa10` checks available space on the
   target filesystem and refuses if the operation would leave **less than
   2 GiB** free.

4. **Unconfirmed size cap.** Output larger than **10 GiB** requires `--confirm`.

5. **Batch limit.** Operating on **more than 100 files** in one invocation
   requires `--batch`.

6. **Pure local FS.** As above: no network, registry, autostart, or
   self-modification.

7. **Clear startup banner.** Unless `--quiet` is passed, `fa10` prints a short
   banner stating what it does and that it is local-filesystem-only.

## Integrity guarantees

- The original content is hashed with **SHA-256**; the hash is stored in the
  footer and re-checked on `restore` (and on `grow --verify`). A mismatch aborts
  the restore and removes the partial output.
- The footer carries a **CRC32** over its own bytes so truncation or corruption
  of the metadata is detected before restoration is attempted.

## Limitations

- The path blocklist is a **best-effort** convenience guard, not a security
  boundary. Do not rely on it to contain untrusted input or untrusted users.
- `fa10` trusts the local filesystem's reporting of free space and file sizes.
