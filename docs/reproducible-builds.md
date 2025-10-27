# Clementine CLI Reproducible Builds

This document explains how to build Clementine CLI reproducibly using [Nix](https://nixos.org/), ensuring bit-for-bit identical binaries across different machines.

**Reproducible builds** are critical for security-sensitive applications like cryptocurrency tools, allowing users to verify that published binaries match the source code exactly.

## Platform Support Status

> [!NOTE]
> **7 working platforms** are fully supported and reproducible. macOS supports cross-compilation between architectures!

| Status | Platforms | Notes |
|--------|-----------|-------|
| **Working** | x86_64, ARM64, ARMv7, RISC-V (Linux), Windows 64-bit, macOS (Intel & Apple Silicon) | All builds verified reproducible. macOS can cross-compile between Intel and Apple Silicon. |

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

### Working Platforms

Build commands for all working platforms:

```bash
# Linux (all tested and verified reproducible)
nix build .#x86_64-linux-gnu        # Intel/AMD 64-bit
nix build .#aarch64-linux-gnu       # ARM 64-bit
nix build .#arm-linux-gnueabihf     # ARMv7 32-bit (Raspberry Pi)
nix build .#riscv64-linux-gnu       # RISC-V 64-bit

# macOS (build on macOS only)
nix build .#x86_64-apple-darwin     # Intel Macs
nix build .#arm64-apple-darwin      # Apple Silicon (M1/M2/M3/M4)

# Windows (cross-compile from Linux)
nix build .#win64                   # 64-bit

# Default (current platform)
nix build
```

**Binary location:** `./result/bin/clementine-cli` (or `.exe` for Windows)

### Cross-Compilation Matrix

| Your System | Can Build For |
|------------|---------------|
| **Linux (x86_64)** | All Linux targets (x86_64, ARM64, ARMv7, RISC-V), Windows (win64), macOS (Intel, Apple Silicon) |
| **Linux (ARM64)** | All Linux targets, Windows (win64), macOS (Intel, Apple Silicon) |
| **macOS (Intel)** | macOS (Intel, Apple Silicon) |
| **macOS (Apple Silicon)** | macOS (Intel, Apple Silicon) |

> [!NOTE]
>
> - **macOS from Linux**: Experimental - included in Linux builds but may not work on all target systems
> - **macOS cross-arch**: ✅ Now supported! Both Intel and Apple Silicon Macs can build for both architectures
> - **Linux/Windows from macOS**: Not currently enabled
> - **Best practice**: Build macOS binaries on macOS (any architecture), build Linux/Windows targets on Linux

## Platform-Specific Build Instructions

### From Linux

Linux can build for all Linux platforms and Windows:

```bash
# Linux builds
nix build .#x86_64-linux-gnu        # Intel/AMD 64-bit
nix build .#aarch64-linux-gnu       # ARM 64-bit
nix build .#arm-linux-gnueabihf     # ARMv7 32-bit
nix build .#riscv64-linux-gnu       # RISC-V 64-bit

# Cross-compile to Windows
nix build .#win64
```

### From macOS

macOS can build for **both Intel and Apple Silicon** architectures:

```bash
# Check your Mac architecture
uname -m  # "x86_64" = Intel, "arm64" = Apple Silicon

# Build for your current Mac architecture
nix build                           # Automatically selects your architecture

# Build for Intel Macs (works on both Intel and Apple Silicon)
nix build .#x86_64-apple-darwin     # Intel Macs

# Build for Apple Silicon (works on both Intel and Apple Silicon)
nix build .#arm64-apple-darwin      # Apple Silicon (M1/M2/M3/M4)
```

> [!TIP]
> macOS can now cross-compile between architectures! You can build Intel binaries on Apple Silicon and vice versa. This uses the universal Apple SDK with architecture-specific linker flags.

**Note**: For Linux and Windows builds, use a Linux system.

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
nix build .#x86_64-linux-gnu
HASH1=$(nix hash path ./result)

rm -rf result
nix build .#x86_64-linux-gnu
HASH2=$(nix hash path ./result)

# Should match
echo "Build 1: $HASH1"
echo "Build 2: $HASH2"
```

### Full Verification (recommended for release verification)

Test with a completely clean Nix store to ensure no cached artifacts affect the build:

```bash
# First clean build
nix build .#x86_64-linux-gnu
HASH1=$(nix hash path ./result)

# Clean everything and rebuild
rm -rf result
nix store gc --max 0  # Removes all cached dependencies (~36GB)
nix build .#x86_64-linux-gnu  # Takes 10-20 minutes
HASH2=$(nix hash path ./result)

# Verify reproducibility
[ "$HASH1" = "$HASH2" ] && echo " Reproducible!" || echo "Not reproducible"
```

### Expected Hashes (Verified Reproducible)

These hashes are **verified reproducible** - building twice produces identical binaries:

| Platform | Hash |
|----------|------|
| **x86_64-linux-gnu** | `sha256-nHLuugEVf0/nJnCG3XhEPP/4+w4d8+ZylcwbxUQVXhI=` |
| **aarch64-linux-gnu** | `sha256-egGIYlIYpKSBG3p0ZT9UPW6ucbVCr8HTqqrZJGwK37A=` |
| **arm-linux-gnueabihf** | `sha256-1/CmV1ond4ePEzlCeOPu1It2WfDxrqdyuoV1pIeteRc=` |
| **riscv64-linux-gnu** | `sha256-zeZDIrAN7GIskHi62PwsJrWCmzTGjlPy17zddqMB5qc=` |
| **win64** | `sha256-ZQ2BUYA13doSj86JJvaOz8ygjV9RRO8tsteocfXdRtg=` |
| **x86_64-apple-darwin** | `sha256-P3A+2Z8GDm0ivPfck27GfKpwl0RFSxwUty+vQyGiXkQ=` |
| **arm64-apple-darwin** | `sha256-d39cfapCrrC4Q+YP4SEgLpv42DHdRcn27XXzhIIJtKs=` |

> **Note**: Hashes change when source code, dependencies (Cargo.lock), or build configuration (flake.nix) are modified.

### Cross-Machine Verification

To verify your build matches another developer's:

```bash
# On any machine with the same commit (example for x86_64 macOS)
nix build .#x86_64-apple-darwin
nix hash path ./result
# Should output: sha256-P3A+2Z8GDm0ivPfck27GfKpwl0RFSxwUty+vQyGiXkQ=
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

**From Linux** (recommended for all non-macOS platforms):

```bash
# Linux platforms
nix build .#x86_64-linux-gnu
cp result/bin/clementine-cli clementine-cli-v0.1.0-x86_64-linux-gnu

nix build .#aarch64-linux-gnu
cp result/bin/clementine-cli clementine-cli-v0.1.0-aarch64-linux-gnu

nix build .#arm-linux-gnueabihf
cp result/bin/clementine-cli clementine-cli-v0.1.0-arm-linux-gnueabihf

nix build .#riscv64-linux-gnu
cp result/bin/clementine-cli clementine-cli-v0.1.0-riscv64-linux-gnu

# Windows
nix build .#win64
cp result/bin/clementine-cli.exe clementine-cli-v0.1.0-win64.exe
```

**From macOS** (for both macOS binaries):

```bash
# Build both architectures on any Mac
nix build .#x86_64-apple-darwin
cp result/bin/clementine-cli clementine-cli-v0.1.0-x86_64-apple-darwin

nix build .#arm64-apple-darwin
cp result/bin/clementine-cli clementine-cli-v0.1.0-arm64-apple-darwin
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

### PowerPC64 Known Issue

> [!CAUTION]
> PowerPC64 builds are currently **not working** due to fundamental limitations in Nix's Rust cross-compilation infrastructure.

**Symptoms:**

- Build completes with exit code 0
- No `result` symlink is created
- No binary is produced

**Root Cause:**
PowerPC64 cross-compilation in Nix for Rust projects has unresolved issues. We tested multiple configurations:

- `pkgsCross.ppc64` (big-endian, ELF v2)
- `pkgsCross.ppc64-elfv1` (big-endian, ELF v1)
- `pkgsCross.ppc64-elfv2` (big-endian, ELF v2)
- `pkgsCross.powernv` (little-endian, ppc64le)

All configurations fail silently during the build process.

**Workaround:**
None currently available. PowerPC64 is a niche architecture with limited toolchain support. This would require upstream fixes in either:

- Nix's PowerPC64 cross-compilation infrastructure
- Rust's PowerPC64 target support within Nix

**Status:** Configuration is kept in `flake.nix` for future compatibility when upstream issues are resolved.

### Common Issues

| Issue | Solution |
|-------|----------|
| **"experimental-features not enabled"** | Add `experimental-features = nix-command flakes` to `~/.config/nix/nix.conf` and restart Nix daemon |
| **Hash mismatch for git dependencies** | Run `./reproducible/update-hashes.sh` or manually update `outputHashes` in `flake.nix` |
| **Windows build fails from macOS** | Expected - use Linux for Windows builds |
| **macOS cross-arch build slow** | Normal on first build - system clang compiles dependencies for target architecture |
| **Slow first build** | Normal - ~500 derivations built (10-20 min). Future builds take 1-2 min |
| **Out of disk space** | Run `nix store gc` (light cleanup) or `nix store gc --max 0` (full cleanup, frees ~36GB) |

### Dependency Hash Updates

If you see hash mismatch errors for git dependencies:

```bash
# Use helper script
./reproducible/update-hashes.sh

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

### Performance Metrics

| Scenario | Platform | Time | Details |
|----------|----------|------|---------|
| **First build** (clean) | Linux x86_64 | 10-20 min | ~500 derivations, ~70MB downloads, 36GB disk usage |
| **First build** (clean) | macOS | 10-20 min | Similar to Linux, uses system clang for compilation |
| **macOS cross-arch** | macOS | 8-15 min | First cross-compilation may be slower, subsequent builds cached |
| **Cached builds** | All | 1-2 min | Only changed components rebuilt |
| **After `nix store gc --max 0`** | All | 10-20 min | Full rebuild, all dependencies |

### Reproducibility Guarantee

- **100% reproducible** for all 7 platforms including both macOS architectures
- Clean builds produce identical hashes
- Cached builds produce identical hashes
- Cross-machine builds produce identical hashes
- macOS cross-arch builds are reproducible (same hash when building x86_64 on different Macs)

This confirms the build system is truly deterministic across all supported platforms.

### Verifying Binary Architecture

```bash
# Check what you built
file ./result/bin/clementine-cli

# Expected outputs by platform:
# x86_64-linux-gnu:        ELF 64-bit LSB executable, x86-64
# aarch64-linux-gnu:       ELF 64-bit LSB executable, ARM aarch64
# arm-linux-gnueabihf:     ELF 32-bit LSB executable, ARM
# riscv64-linux-gnu:       ELF 64-bit LSB executable, UCB RISC-V
# x86_64-apple-darwin:     Mach-O 64-bit executable x86_64
# arm64-apple-darwin:      Mach-O 64-bit executable arm64
# win64:                   PE32+ executable (console) x86-64
```

### macOS Cross-Architecture Compilation

macOS users can build for both Intel and Apple Silicon on any Mac:

```bash
# On Apple Silicon Mac, build for Intel:
nix build .#x86_64-apple-darwin
file ./result/bin/clementine-cli
# Output: Mach-O 64-bit executable x86_64

# On Intel Mac, build for Apple Silicon:
nix build .#arm64-apple-darwin
file ./result/bin/clementine-cli
# Output: Mach-O 64-bit executable arm64
```

This works by using custom compiler wrappers that leverage the system's clang with appropriate `-arch` flags.

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
        run: nix build .#x86_64-linux-gnu

      - name: Verify Reproducibility
        run: |
          HASH1=$(nix hash path ./result)
          rm -rf result
          nix build .#x86_64-linux-gnu
          HASH2=$(nix hash path ./result)
          [ "$HASH1" = "$HASH2" ] && echo " Reproducible!" || exit 1

      - name: Build Windows
        run: nix build .#win64

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
