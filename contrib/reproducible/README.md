# Clementine CLI Reproducible Builds

This directory contains scripts and configuration for building Clementine CLI in a reproducible manner using [Nix](https://nixos.org/), a functional package manager.

Reproducible builds ensure that compiling the same source code produces bit-for-bit identical binaries, allowing users to verify that published binaries match the source code. This is crucial for security-sensitive applications like cryptocurrency tools.

## Prerequisites

### Installing Nix

You will need to install [Nix](https://nixos.org/download/#download-nix), a package manager.

#### Enable Flakes

We rely on Nix flakes, so you need to enable this experimental feature by adding to your `~/.config/nix/nix.conf`:

```
experimental-features = nix-command flakes
```

If the directory doesn't exist, create it:

```bash
mkdir -p ~/.config/nix
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
```

Restart the Nix daemon after making this change:

```bash
# On Linux
sudo systemctl restart nix-daemon

# On macOS
sudo launchctl unload /Library/LaunchDaemons/org.nixos.nix-daemon.plist
sudo launchctl load /Library/LaunchDaemons/org.nixos.nix-daemon.plist
```

## Building for Different Platforms

All builds are performed from the repository root.

### Linux (x86_64)

```bash
nix build .#packages.x86_64-linux.default
```

The binary will be in `./result/bin/clementine-cli`.

### Linux (ARM64)

```bash
nix build .#packages.aarch64-linux.default
```

### macOS

#### Prerequisites for macOS Cross-Compilation (from Linux)

If you want to build macOS binaries from Linux, you first need to obtain the macOS SDK. The Xcode 12.2 version is required (`Xcode_12.2.xip`).

**Important**: You need to download this from Apple's website. An Apple ID is required (you can create one for free). Note that it is illegal to distribute this archive.

1. **Download Xcode 12.2**:
   - Use the [direct link](https://download.developer.apple.com/Developer_Tools/Xcode_12.2/Xcode_12.2.xip)
   - Or go to [Apple Downloads](https://developer.apple.com/download/all/?q=Xcode%2012.2), then 'More' and search for Xcode 12.2
   - Verify the SHA256 checksum:
     ```bash
     sha256sum Xcode_12.2.xip
     # Should be: 28d352f8c14a43d9b8a082ac6338dc173cb153f964c6e8fb6ba389e5be528bd0
     ```

2. **Extract the SDK and add to Nix store**:
   ```bash
   nix run github:edouardparis/unxip#unxip -- Xcode_12.2.xip Xcode_12.2
   cd Xcode_12.2
   nix-store --add-fixed --recursive sha256 Xcode.app
   ```
   Note: This may take a long time.

#### Building for macOS (from macOS)

If you're already on macOS, you can build natively without the SDK setup:

**Apple Silicon (M1/M2/M3)**:
```bash
nix build .#packages.aarch64-darwin.default
```

**Intel**:
```bash
nix build .#packages.x86_64-darwin.default
```

#### Cross-compiling for macOS (from Linux)

After setting up the macOS SDK:

**Apple Silicon**:
```bash
nix build .#packages.aarch64-darwin.default
```

**Intel**:
```bash
nix build .#packages.x86_64-darwin.default
```

### Windows (x86_64)

**Note**: This requires building from a Linux system with cross-compilation support.

```bash
nix build .#packages.x86_64-windows.default
```

The binary will be `./result/bin/clementine-cli.exe`.

## Development Environment

Enter a development shell with all dependencies:

```bash
nix develop
```

This provides a shell with Rust 1.89.0 and all required build dependencies.

## Verifying Reproducibility

To verify that builds are reproducible:

1. Build on one machine and note the hash:
   ```bash
   nix build .#packages.x86_64-linux.default
   nix hash path ./result
   ```

2. Build on a different machine (or remove and rebuild):
   ```bash
   rm -rf result
   nix build .#packages.x86_64-linux.default
   nix hash path ./result
   ```

3. Compare the hashes - they should be identical.

## Release Process

For release managers preparing official builds:

1. **Tag the release**:
   ```bash
   git tag -s v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

2. **Build for all platforms**:
   ```bash
   # Linux x86_64
   nix build .#packages.x86_64-linux.default
   cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-x86_64

   # Linux ARM64
   nix build .#packages.aarch64-linux.default
   cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-aarch64

   # macOS Apple Silicon
   nix build .#packages.aarch64-darwin.default
   cp result/bin/clementine-cli clementine-cli-v0.1.0-macos-aarch64

   # macOS Intel
   nix build .#packages.x86_64-darwin.default
   cp result/bin/clementine-cli clementine-cli-v0.1.0-macos-x86_64

   # Windows (from Linux)
   nix build .#packages.x86_64-windows.default
   cp result/bin/clementine-cli.exe clementine-cli-v0.1.0-windows-x86_64.exe
   ```

3. **Generate checksums**:
   ```bash
   sha256sum clementine-cli-v0.1.0-* > SHA256SUMS.txt
   ```

4. **Sign the checksums** (requires GPG key):
   ```bash
   gpg --clearsign SHA256SUMS.txt
   ```

5. **Upload to GitHub releases** with the signed checksums.

## First-Time Setup

### Updating Git Dependency Hashes

On your first build, you'll need to update the hashes for git dependencies. We provide a helper script:

```bash
./contrib/reproducible/update-hashes.sh
```

This script will:
1. Attempt to build the project
2. Capture any hash mismatch errors
3. Display the correct hashes you need to update in `flake.nix`

Alternatively, you can manually:
1. Try to build: `nix build .#packages.x86_64-linux.default`
2. Note the error message with the expected hash
3. Update the `outputHashes` section in `flake.nix` with the correct hashes
4. Rebuild

## Troubleshooting

### Hash Mismatch Errors

If you encounter errors about hash mismatches for git dependencies:

Example error:
```
error: hash mismatch in fixed-output derivation
  specified: sha256-AAAAAAA...
  got:       sha256-Bxxxxxx...
```

Update `flake.nix`:
```nix
outputHashes = {
  "bitcoincore-rpc-0.18.0" = "sha256-Bxxxxxx...";
  "secp256k1-0.31.0" = "sha256-Cxxxxxx...";
};
```

### Cross-Compilation Issues

If Windows cross-compilation fails, ensure you're building from a Linux system with MinGW support available through Nix.

### Cache Issues

If you encounter caching issues, you can clear the Nix store cache:

```bash
nix store gc
```

## Learn More

- [Reproducible Builds](https://reproducible-builds.org/) - About reproducible builds
- [Nix Manual](https://nixos.org/manual/nix/stable/) - Complete Nix documentation
- [Nix Flakes](https://nixos.wiki/wiki/Flakes) - About Nix flakes
