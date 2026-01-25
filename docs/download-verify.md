# Download and Verify Clementine CLI

This guide explains how to download pre-built binaries and verify their
integrity using SHA256 checksums and PGP signatures.

## Release Artifacts

Each GitHub release includes these assets:

- Platform-specific binaries named by release tag and OS/arch, for example
  `clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>` or
  `clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe`.
- `SHA256SUMS` file with checksums for all release binaries.
- `SHA256SUMS.asc` which is the PGP-signed checksum file.

Example release:
`https://github.com/chainwayxyz/clementine-cli/releases/tag/v0.1.0-rc.1`

## Download

- Download the release binary for your platform.

Download verification is optional but strongly recommended. These steps ensure
the binaries and checksum files you downloaded are authentic and unmodified.

- Download `SHA256SUMS`.
- Download `SHA256SUMS.asc`.
- Keep all three files together in the folder where the release artifacts were
  downloaded.

## Import the Release Signing Key

Only trust a key after validating the **full fingerprint** out-of-band.

If GPG is not installed, install it before proceeding:

- macOS: `https://gpgtools.org`
- Linux: your package manager
- Windows: Gpg4win (`https://gpg4win.org/download.html`)

References:

- Keyserver: `hkps://keyserver.ubuntu.com`
- Public key repository: `https://github.com/chainwayxyz/pgp-keys`
- Full fingerprints: `https://github.com/chainwayxyz/pgp-keys/blob/main/FINGERPRINTS.md`
- Signer keys: `https://github.com/chainwayxyz/pgp-keys/tree/main/clementine-cli-builder`

### macOS/Linux

#### From a keyserver

```sh
gpg --keyserver hkps://keyserver.ubuntu.com --recv-keys <KEY_FINGERPRINT>
gpg --fingerprint <KEY_FINGERPRINT>
```

Use the fingerprint or key ID with no spaces in the `recv-keys` command.

Expected output:

- `gpg` reports the key was retrieved/imported.
- The `gpg --fingerprint` line shows the full fingerprint and matches
  `FINGERPRINTS.md`.

#### From the public key repository

Select a trusted signer from the `clementine-cli-builder` directory and use
that filename as `<KEY_FILENAME>`. For stronger assurance, verify against
multiple trusted signers and compare fingerprints before trusting a key.

```sh
curl -fsSL https://raw.githubusercontent.com/chainwayxyz/pgp-keys/main/clementine-cli-builder/<KEY_FILENAME> -o clementine-cli-release.pgp
gpg --import clementine-cli-release.pgp
gpg --fingerprint <KEY_FINGERPRINT>
```

Expected output:

- `gpg` reports the key was imported.
- The `gpg --fingerprint` line shows the full fingerprint and matches
  `FINGERPRINTS.md`.

#### Import all signer keys (optional)

```sh
git clone https://github.com/chainwayxyz/pgp-keys.git
gpg --import pgp-keys/clementine-cli-builder/*.pgp
```

Expected output:

- `gpg` reports each key import.
- The fingerprints you intend to trust match `FINGERPRINTS.md`.

### Windows (PowerShell)

#### From a keyserver

```powershell
& "C:\Program Files (x86)\GnuPG\bin\gpg.exe" --keyserver hkps://keyserver.ubuntu.com --recv-keys <KEY_FINGERPRINT>
& "C:\Program Files (x86)\GnuPG\bin\gpg.exe" --fingerprint <KEY_FINGERPRINT>
```

Command Prompt:

```cmd
"C:\Program Files (x86)\GnuPG\bin\gpg.exe" --keyserver hkps://keyserver.ubuntu.com --recv-keys <KEY_FINGERPRINT>
"C:\Program Files (x86)\GnuPG\bin\gpg.exe" --fingerprint <KEY_FINGERPRINT>
```

Use the fingerprint or key ID with no spaces in the `recv-keys` command.

Expected output:

- `gpg` reports the key was retrieved/imported.
- The `gpg --fingerprint` line shows the full fingerprint and matches
  `FINGERPRINTS.md`.

#### From the public key repository

Select a trusted signer from the `clementine-cli-builder` directory and use
that filename as `<KEY_FILENAME>`. For stronger assurance, verify against
multiple trusted signers and compare fingerprints before trusting a key.

```powershell
curl.exe -fsSL https://raw.githubusercontent.com/chainwayxyz/pgp-keys/main/clementine-cli-builder/<KEY_FILENAME> -o clementine-cli-release.pgp
& "C:\Program Files (x86)\GnuPG\bin\gpg.exe" --import clementine-cli-release.pgp
& "C:\Program Files (x86)\GnuPG\bin\gpg.exe" --fingerprint <KEY_FINGERPRINT>
```

Command Prompt:

```cmd
curl -fsSL https://raw.githubusercontent.com/chainwayxyz/pgp-keys/main/clementine-cli-builder/<KEY_FILENAME> -o clementine-cli-release.pgp
"C:\Program Files (x86)\GnuPG\bin\gpg.exe" --import clementine-cli-release.pgp
"C:\Program Files (x86)\GnuPG\bin\gpg.exe" --fingerprint <KEY_FINGERPRINT>
```

Expected output:

- `gpg` reports the key was imported.
- The `gpg --fingerprint` line shows the full fingerprint and matches
  `FINGERPRINTS.md`.

Replace `<KEY_FILENAME>` with the specific signer key file (for example,
`ahmet-oguz-engin.pgp`) to avoid ambiguity.

#### Import all signer keys (optional)

```powershell
git clone https://github.com/chainwayxyz/pgp-keys.git
& "C:\Program Files (x86)\GnuPG\bin\gpg.exe" --import pgp-keys\clementine-cli-builder\*.pgp
```

Command Prompt:

```cmd
git clone https://github.com/chainwayxyz/pgp-keys.git
"C:\Program Files (x86)\GnuPG\bin\gpg.exe" --import pgp-keys\clementine-cli-builder\*.pgp
```

Expected output:

- `gpg` reports each key import.
- The fingerprints you intend to trust match `FINGERPRINTS.md`.

The public key repository should contain:

- Individual `.pgp` key files under `clementine-cli-builder/`
- `FINGERPRINTS.md` to cross-check expected fingerprints

## Verify the Checksum Signature

### macOS/Linux

- Run these commands from the folder where the release artifacts were
  downloaded.
- Ensure the trusted signer keys are imported and the fingerprints match
  `FINGERPRINTS.md`.
- Verify the checksum signature:

```sh
gpg --verify SHA256SUMS.asc SHA256SUMS
```

### Windows (PowerShell)

- Run these commands from the folder where the release artifacts were
  downloaded:
  - PowerShell: `cd $env:USERPROFILE\Downloads`
  - Command Prompt: `cd %UserProfile%\Downloads`
- Ensure the trusted signer keys are imported and the fingerprints match
  `FINGERPRINTS.md`.
- Verify the checksum signature:

```powershell
& "C:\Program Files (x86)\GnuPG\bin\gpg.exe" --verify SHA256SUMS.asc SHA256SUMS
```

Command Prompt:

```cmd
"C:\Program Files (x86)\GnuPG\bin\gpg.exe" --verify SHA256SUMS.asc SHA256SUMS
```

If Gpg4win installed to a different path (for example,
`C:\Program Files\GnuPG\bin\gpg.exe`), adjust the command accordingly.

Expected output:

- A line that starts with: `gpg: Good signature`
- A line that includes: `Primary key fingerprint: ...`

The fingerprint shown by GPG must match one of the trusted fingerprints you
validated from `https://github.com/chainwayxyz/pgp-keys/blob/main/FINGERPRINTS.md`.
If the signer differs from your trusted set, treat it as untrusted and stop.

You may also see warnings:

- `gpg: Can't check signature: No public key` means you have not imported that
  signer. If you trust only a subset of signers and have their keys, this can
  be ignored.
- `gpg: WARNING: This key is not certified with a trusted signature!` or
  `WARNING: The key's User ID is not certified with a trusted signature!`
  means GPG cannot establish trust. Confirm the fingerprint matches a trusted
  signer before proceeding.

Proceed only if the signature is valid and the fingerprint matches your trusted
key record.

## Verify the Binary Checksum

Use the checksum file to verify the binary you downloaded. Follow the section
for your OS.

### macOS/Linux

From the folder where the release artifacts were downloaded, run:

```sh
sha256sum --check --ignore-missing --strict SHA256SUMS
```

Expected output:

- The line for your downloaded file ends with `OK`
  (for example: `clementine-cli-v0.1.0-rc.1-darwin-aarch64: OK`).

To compute a hash directly for auditing or tooling and compare it to the
matching line in `SHA256SUMS`:

```sh
sha256sum clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>
```

Expected output:

- A single SHA256 hash and filename; it must match the corresponding line in
  `SHA256SUMS`.

### Windows (PowerShell)

From the folder where the release artifacts were downloaded, run:

```powershell
$pattern = '^[0-9a-fA-F]{64}\s+\*?clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>\.exe$'
$expected = ((Select-String -Path SHA256SUMS -Pattern $pattern).Line -split '\s+')[0]
$actual = (Get-FileHash .\clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe -Algorithm SHA256).Hash
$expected -eq $actual
```

Expected output:

- `True` when the checksum matches.

### Windows (Command Prompt)

From the folder where the release artifacts were downloaded, run:

```cmd
certutil -hashfile clementine-cli-<RELEASE_TAG>-windows-x86_64.exe SHA256
type SHA256SUMS
```

Expected output:

- The SHA256 value from `certutil` matches the corresponding line in
  `SHA256SUMS` (compare every character).

To compute a hash directly for auditing or tooling and compare it to the
matching line in `SHA256SUMS`:

```powershell
Get-FileHash .\clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe -Algorithm SHA256
```

Expected output:

- The `Hash` value matches the corresponding line in `SHA256SUMS`.

## Rename and Run

For easier usage, rename the binary to `clementine-cli` and ensure it is
executable. On macOS/Linux, use `./` because the current directory is not in
PATH by default, or add the directory containing the binary to your PATH for
global access.

### macOS/Linux

```sh
mv clementine-cli-<RELEASE_TAG>-<OS>-<ARCH> clementine-cli
chmod +x clementine-cli
./clementine-cli --help
```

### Windows (PowerShell)

```powershell
Rename-Item clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe clementine-cli.exe
.\clementine-cli.exe --help
```

### Windows (Command Prompt)

```cmd
rename clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe clementine-cli.exe
clementine-cli.exe --help
```
