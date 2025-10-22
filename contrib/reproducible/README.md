# Clementine CLI Reproducible Builds

This directory contains scripts and configuration for building Clementine CLI in a reproducible manner using [Nix](https://nixos.org/), a functional package manager.

Reproducible builds ensure that compiling the same source code produces bit-for-bit identical binaries, allowing users to verify that published binaries match the source code. This is crucial for security-sensitive applications like cryptocurrency tools.

## Quick Start

### 1. Install Nix

Install [Nix](https://nixos.org/download/#download-nix) if you haven't already:

```bash
sh <(curl -L https://nixos.org/nix/install) --daemon
```

### 2. Enable Flakes (if using official installer)

If you used the official Nix installer, enable flakes:

```bash
mkdir -p ~/.config/nix
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf

# Restart Nix daemon
# On Linux:
sudo systemctl restart nix-daemon

# On macOS:
sudo launchctl unload /Library/LaunchDaemons/org.nixos.nix-daemon.plist
sudo launchctl load /Library/LaunchDaemons/org.nixos.nix-daemon.plist
```

### 3. Build

```bash
# Build for your current platform
nix build

# Or build for a specific platform
nix build .#x86_64-linux     # Linux x86_64
nix build .#x86_64-windows   # Windows x86_64

# Find your binary
ls -la ./result/bin/
```

**First build?** Expect 10-20 minutes as Nix builds ~500 dependencies. Subsequent builds take 1-2 minutes.

## Supported Platforms

Build commands for all supported platforms:

```bash
# Linux
nix build .#x86_64-linux        # Intel/AMD 64-bit
nix build .#aarch64-linux       # ARM 64-bit

# macOS
nix build .#x86_64-darwin       # Intel Macs
nix build .#aarch64-darwin      # Apple Silicon (M1/M2/M3/M4)

# Windows
nix build .#x86_64-windows      # 64-bit (cross-compile from Linux)

# Default (current platform)
nix build
```

**Binary location:** `./result/bin/clementine-cli` (or `.exe` for Windows)

### Cross-Compilation Matrix

| Your System | Can Build For |
|------------|---------------|
| **Linux** | ✅ Linux (x86_64, ARM64)<br/>✅ Windows (x86_64) |
| **macOS** | ✅ macOS (Intel, Apple Silicon)<br/>✅ Linux (x86_64, ARM64)<br/>⚠️ Windows (experimental) |

**Notes:**

- **Windows from macOS**: Experimental, may fail. Use Linux for production Windows builds.
- **Best practice**: Build Windows binaries from Linux for reliability

## Platform-Specific Build Instructions

### From Linux

Linux can build for Linux and Windows:

```bash
# Native Linux builds
nix build .#x86_64-linux     # Your platform (likely)
nix build .#aarch64-linux    # ARM64 Linux

# Cross-compile to Windows (recommended method)
nix build .#x86_64-windows
```

### From macOS

macOS can build for macOS and Linux:

```bash
# Check your Mac architecture
uname -m  # "x86_64" = Intel, "arm64" = Apple Silicon

# Native macOS builds
nix build                    # Current Mac architecture
nix build .#x86_64-darwin    # Intel Macs
nix build .#aarch64-darwin   # Apple Silicon (M1/M2/M3/M4)

# Cross-compile to Linux
nix build .#x86_64-linux
nix build .#aarch64-linux

# Windows (experimental - prefer Linux for production)
nix build .#x86_64-windows   # May fail, use Linux instead
```

## Development Environment

Enter a development shell with all dependencies:

```bash
nix develop
```

This provides a shell with Rust 1.89.0 and all required build dependencies.

## Verifying Reproducibility

### Quick Test

```bash
# Build twice and compare hashes
nix build .#x86_64-linux
HASH1=$(nix hash path ./result)

rm -rf result
nix build .#x86_64-linux
HASH2=$(nix hash path ./result)

# Should match
echo "Build 1: $HASH1"
echo "Build 2: $HASH2"
```

### Full Verification (recommended for release verification)

Test with a completely clean Nix store to ensure no cached artifacts affect the build:

```bash
# First clean build
nix build .#x86_64-linux
HASH1=$(nix hash path ./result)

# Clean everything and rebuild
rm -rf result
nix store gc --max 0  # Removes all cached dependencies (~36GB)
nix build .#x86_64-linux  # Takes 10-20 minutes
HASH2=$(nix hash path ./result)

# Verify reproducibility
[ "$HASH1" = "$HASH2" ] && echo "✅ Reproducible!" || echo "❌ Not reproducible"
```

### Expected Hashes (current commit)

| Platform | Hash |
|----------|------|
| Linux x86_64 | `sha256-QIGF2H6ZcmuAU8mCYvNu0VJbVw3+Kw0eHD7pD/IjST0=` |
| Windows x86_64 | `sha256-QgZRnsaQjGm1h+YSJ8qehI6NrG6KxSGZY9uqQVwFN2o=` |

> **Note**: Hashes change when source code, dependencies (Cargo.lock), or build configuration (flake.nix) are modified.

### Cross-Machine Verification

To verify your build matches another developer's:

```bash
# On any machine with the same commit
nix build .#x86_64-windows
nix hash path ./result
# Should output: sha256-QgZRnsaQjGm1h+YSJ8qehI6NrG6KxSGZY9uqQVwFN2o=
```

If hashes don't match, ensure:
- Same git commit/tag
- Same Nix version (check with `nix --version`)
- Clean build (`nix store gc --max 0` first)

## Release Process

### 1. Tag Release

```bash
git tag -s v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

### 2. Build All Platforms

**From Linux** (recommended for Windows):

```bash
# Linux platforms
nix build .#x86_64-linux
cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-x86_64

nix build .#aarch64-linux
cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-aarch64

# Windows
nix build .#x86_64-windows
cp result/bin/clementine-cli.exe clementine-cli-v0.1.0-windows-x86_64.exe
```

**From macOS** (for macOS binaries):

```bash
# macOS platforms
nix build .#x86_64-darwin
cp result/bin/clementine-cli clementine-cli-v0.1.0-macos-x86_64

nix build .#aarch64-darwin
cp result/bin/clementine-cli clementine-cli-v0.1.0-macos-aarch64

# Can also build Linux from macOS
nix build .#x86_64-linux
cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-x86_64
```

### 3. Generate and Sign Checksums

```bash
# Generate checksums
sha256sum clementine-cli-v0.1.0-* > SHA256SUMS.txt

# Sign (requires GPG key)
gpg --clearsign SHA256SUMS.txt

# Upload to GitHub Releases
# Include: binaries + SHA256SUMS.txt.asc
```

## Troubleshooting

### Common Issues

| Issue | Solution |
|-------|----------|
| **"experimental-features not enabled"** | Add `experimental-features = nix-command flakes` to `~/.config/nix/nix.conf` and restart Nix daemon |
| **Hash mismatch for git dependencies** | Run `./contrib/reproducible/update-hashes.sh` or manually update `outputHashes` in `flake.nix` |
| **Windows build fails from macOS** | Expected - use Linux for Windows builds |
| **Slow first build** | Normal - ~500 derivations built (10-20 min). Future builds take 1-2 min |
| **Out of disk space** | Run `nix store gc` (light cleanup) or `nix store gc --max 0` (full cleanup, frees ~36GB) |

### Dependency Hash Updates

If you see hash mismatch errors for git dependencies:

```bash
# Use helper script
./contrib/reproducible/update-hashes.sh

# Or manually update flake.nix outputHashes section with values from error messages
```

### Cleaning Build Cache

```bash
# Light cleanup (removes unreferenced packages)
nix store gc

# Full cleanup (removes everything, frees ~36GB)
# Warning: Next build will take 10-20 minutes
nix store gc --max 0
```

Use full cleanup when:

- Testing true reproducibility
- Freeing disk space
- Resolving cache corruption

## Build Performance & Reproducibility

### Performance Metrics (Linux x86_64)

| Scenario | Time | Details |
|----------|------|---------|
| **First build** (clean) | 10-20 min | ~500 derivations, ~70MB downloads, 36GB disk usage |
| **Cached builds** | 1-2 min | Only changed components rebuilt |
| **After `nix store gc --max 0`** | 10-20 min | Full rebuild, all dependencies |

### Reproducibility Guarantee

- ✅ **100% reproducible** for Linux and Windows
- ✅ Clean builds produce identical hashes
- ✅ Cached builds produce identical hashes
- ✅ Cross-machine builds produce identical hashes

This confirms the build system is truly deterministic.

### Verifying Binary Architecture

```bash
# Check what you built
file ./result/bin/clementine-cli

# Expected outputs:
# Linux:   ELF 64-bit LSB executable, x86-64
# macOS:   Mach-O 64-bit executable x86_64 (or arm64)
# Windows: PE32+ executable (console) x86-64
```

## CI/CD Integration (Optional)

### GitHub Actions Example

```yaml
name: Reproducible Builds

on: [push, pull_request]

jobs:
  build-and-verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v24

      - name: Build Linux
        run: nix build .#x86_64-linux

      - name: Verify Reproducibility
        run: |
          HASH1=$(nix hash path ./result)
          rm -rf result
          nix build .#x86_64-linux
          HASH2=$(nix hash path ./result)
          [ "$HASH1" = "$HASH2" ] && echo "✅ Reproducible!" || exit 1

      - name: Build Windows
        run: nix build .#x86_64-windows

      - name: Upload Artifacts
        uses: actions/upload-artifact@v4
        with:
          name: binaries
          path: result/bin/*
```

### Benefits

- Automatic reproducibility verification on every commit
- Multi-platform builds in one workflow
- Guaranteed consistency between dev and release builds
- Hash verification before publishing

## Additional Resources

- [Reproducible Builds Project](https://reproducible-builds.org/)
- [Nix Documentation](https://nixos.org/manual/nix/stable/)
- [Nix Flakes Guide](https://nixos.wiki/wiki/Flakes)
- [Cachix (Nix Binary Cache)](https://cachix.org/)
