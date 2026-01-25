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

1. Identify the binary for your OS/arch.
2. Download the matching binary, `SHA256SUMS`, and `SHA256SUMS.asc` from the
   release assets.

Download verification is optional but strongly recommended. These steps ensure
the binaries and checksum files you downloaded are authentic and unmodified.

- Keyserver: `keyserver.ubuntu.com`
- Public key repository: `https://github.com/chainwayxyz/pgp-keys`

## Import the Release Signing Key

Only trust a key after validating the **full fingerprint** out-of-band.

### Option A: Import from a keyserver

Full fingerprints are listed at:
`https://github.com/chainwayxyz/pgp-keys/blob/main/FINGERPRINTS.md`

```sh
gpg --keyserver keyserver.ubuntu.com --recv-keys <KEY_FINGERPRINT>
gpg --fingerprint <KEY_FINGERPRINT>
```

Expected output:

- `gpg` reports the key was retrieved/imported.
- The `gpg --fingerprint` line shows the full fingerprint and matches
  `FINGERPRINTS.md`.

### Option B: Import from the public key repository

Select a trusted signer from:
`https://github.com/chainwayxyz/pgp-keys/tree/main/clementine-cli-builder`
and use that filename as `<KEY_FILENAME>`.
For stronger assurance, verify against multiple trusted signers and compare
fingerprints before trusting a key. Full fingerprints are listed at:
`https://github.com/chainwayxyz/pgp-keys/blob/main/FINGERPRINTS.md`

```sh
curl -fsSL https://raw.githubusercontent.com/chainwayxyz/pgp-keys/main/clementine-cli-builder/<KEY_FILENAME> -o clementine-cli-release.pgp
gpg --import clementine-cli-release.pgp
gpg --fingerprint <KEY_FINGERPRINT>
```

Expected output:

- `gpg` reports the key was imported.
- The `gpg --fingerprint` line shows the full fingerprint and matches
  `FINGERPRINTS.md`.

Replace `<KEY_FILENAME>` with the specific signer key file (for example,
`ahmet-oguz-engin.pgp`) to avoid ambiguity.

You can import all keys at once by cloning the repo and importing the directory:

```sh
git clone https://github.com/chainwayxyz/pgp-keys.git
gpg --import pgp-keys/clementine-cli-builder/*.pgp
```

Expected output:

- `gpg` reports each key import.
- The fingerprints you intend to trust match `FINGERPRINTS.md`.

After importing, validate the fingerprints against
`https://github.com/chainwayxyz/pgp-keys/blob/main/FINGERPRINTS.md`.

The public key repository should contain:

- Individual `.pgp` key files under `clementine-cli-builder/`
- `FINGERPRINTS.md` to cross-check expected fingerprints

## Verify the Checksum Signature

```sh
gpg --verify SHA256SUMS.asc SHA256SUMS
```

Expected output:

- A line that starts with: `gpg: Good signature`
- A line that includes: `Primary key fingerprint: E777 299F C265 DD04 7930  70EB 944D 35F9 AC3D B76A`

You may also see warnings:

- `gpg: Can't check signature: No public key` means you have not imported that
  signer. If you trust only a subset of signers and have their keys, this can
  be ignored.
- `gpg: WARNING: This key is not certified with a trusted signature!` or
  `WARNING: The key's User ID is not certified with a trusted signature!`
  means GPG cannot establish trust. Confirm the fingerprint matches what you
  expect from `FINGERPRINTS.md` for the signer you trust.

Proceed only if the signature is valid and the fingerprint matches your trusted
key record.

## Verify the Binary Checksum

Use the checksum file to verify the binary you downloaded. Follow the section
for your OS.

### macOS/Linux

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

Expected output:

- The line for your downloaded file ends with `OK`
  (for example: `clementine-cli-v0.1.0-rc.1-darwin-aarch64: OK`).

If `sha256sum` is not available (common on macOS), use:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
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

```powershell
$expected = ((Select-String -Path SHA256SUMS -Pattern "clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe").Line -split '\s+')[0]
$actual = (Get-FileHash .\clementine-cli-<RELEASE_TAG>-<OS>-<ARCH>.exe -Algorithm SHA256).Hash
$expected -eq $actual
```

Expected output:

- `True` when the checksum matches.

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
