# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately via GitHub Security Advisories
("Report a vulnerability" on the repository's **Security** tab) rather than a
public issue. We aim to acknowledge reports within a few days.

## Threat model and design intent

`fa10` is a **local-filesystem CLI**. It packs files and directories into a
`.fa10` archive and extracts that archive back, on the same machine. It is not a
sandbox and makes no attempt to defend against an attacker who already controls
the machine or the files it is pointed at. Its safety features exist to prevent
accidental footguns, such as a mistyped path, an unintended overwrite, or a
runaway size. They are not meant to contain hostile input.

What `fa10` deliberately does **not** do:

- **No network.** It opens no sockets and contacts no servers. The crate has no
  HTTP/TLS dependencies.
- **No persistence beyond the output file.** It writes no config, cache,
  registry keys, autostart entries, or hidden state.
- **No self-modification or code execution.** It never rewrites its own binary,
  loads plugins, or executes external commands.

## Safety properties

These guardrails are enforced before any bytes are written:

1. **Inputs are never modified by default.** Output goes to a sibling file
   (`name.fa10` / `dir.fa10`). An input is only overwritten when **both**
   `--in-place` and `--confirm` are supplied (single file only), and even then
   the new file is written to a temporary path and atomically renamed into place.

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

5. **Batch limit.** Packing **more than 100 files** in one archive requires
   `--batch`.

6. **Extraction stays inside the target directory.** On `restore`, every
   manifest path is rejected if it is absolute, drive-qualified, or contains a
   `..` component (the Zip-Slip guard), so a crafted archive cannot write
   outside the extraction directory. Existing files are not overwritten without
   `--force`. Archives never contain symlinks (links are followed and stored as
   regular files at pack time), so extraction only creates regular files and
   directories.

7. **Pure local FS.** As above: no network, registry, autostart, or
   self-modification.

8. **Clear startup banner.** Unless `--quiet` is passed, `fa10` prints a short
   banner stating what it does and that it is local-filesystem-only.

## Integrity guarantees

- Each entry's content is hashed with **SHA-256**; the hash is stored in the
  manifest and re-checked on `restore` (and on `grow --verify`). A mismatch
  aborts and removes the partial output.
- The manifest carries a **CRC32** over its own bytes so truncation or
  corruption of the metadata is detected before extraction is attempted.

## Limitations

- The path blocklist is a **best-effort** convenience guard, not a security
  boundary. Do not rely on it to contain untrusted input or untrusted users.
- `fa10` trusts the local filesystem's reporting of free space and file sizes.
