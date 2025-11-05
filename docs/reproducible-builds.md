# Clementine CLI Reproducible Builds

This guide shows how to build Clementine CLI reproducibly using [Nix](https://nixos.org/) and verify your build matches the official release.

Reproducible builds ensure bit-for-bit identical binaries, allowing you to verify that published binaries match the source code exactly.

## Platform Support

**7 platforms** are fully supported and reproducible:

| Platform | Architecture |
|----------|--------------|
| Linux | x86_64, ARM64, ARMv7, RISC-V |
| Windows | 64-bit |
| macOS | Intel & Apple Silicon |

### What You Can Build

| Your System | Can Build For |
|-------------|---------------|
| **Linux (x86_64 or ARM64)** | All Linux platforms, Windows |
| **macOS (Intel or Apple Silicon)** | Both macOS architectures (Intel ↔ Apple Silicon) |

> [!NOTE]
> **Best practice:** Build macOS binaries on macOS, build Linux/Windows on Linux.

## Install Nix

```bash
sh <(curl -L https://nixos.org/nix/install) --daemon
```

Enable flakes (required):

```bash
mkdir -p ~/.config/nix
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf

# Restart Nix daemon
# Linux:
sudo systemctl restart nix-daemon

# macOS:
sudo launchctl unload /Library/LaunchDaemons/org.nixos.nix-daemon.plist
sudo launchctl load /Library/LaunchDaemons/org.nixos.nix-daemon.plist
```

## Build

```bash
# Build for your platform
nix build

# Or specify a platform:
nix build .#x86_64-linux-gnu      # Linux x86_64
nix build .#aarch64-linux-gnu     # Linux ARM64
nix build .#arm-linux-gnueabihf   # Linux ARMv7 (Raspberry Pi)
nix build .#riscv64-linux-gnu     # Linux RISC-V
nix build .#win64                 # Windows 64-bit
nix build .#x86_64-apple-darwin   # macOS Intel
nix build .#arm64-apple-darwin    # macOS Apple Silicon

# Binary location:
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
| **x86_64-linux-gnu** | `sha256-NCmTVgAjznXctvxE012CjB3C2/by0dBJg5iDsKsHwgE=` |
| **aarch64-linux-gnu** | `sha256-ZlH4xzRwZth8clam1P0FV6YXt9qzf4hwSfkDR7g0lgc=` |
| **arm-linux-gnueabihf** | `sha256-LZw03xTtNGbREMnomn57MYVqVbd/bkNdZUfxuiSPKKE=` |
| **riscv64-linux-gnu** | `sha256-fLpJHlaMFKK2AwX555dbLazTpHKAIXQmq90aUWXnwAg=` |
| **win64** | `sha256-1WhKKBTmyTQr3ukqkohxWFX8HZna20ZAYDVmCFY1R5E=` |
| **x86_64-apple-darwin** | `sha256-p+PlOQNjlKM36kHcIDnopuHBaJHPYkj/mxzgNH86v7U=` |
| **arm64-apple-darwin** | `sha256-fd9F1IcPu1ez8IMQZ3KJu+n7GTgtR/tayQSeAnPtwd8=` |

> **Note**: Hashes change when source code, dependencies (Cargo.lock), or build configuration (flake.nix) are modified.

### Cross-Machine Verification

To verify your build matches another developer's:
**First build takes 10-20 minutes.** Subsequent builds are cached and take 1-2 minutes.

## Verify Your Build

After building, verify your binary matches the expected hash:

```bash
nix hash path ./result
# Should output: sha256-p+PlOQNjlKM36kHcIDnopuHBaJHPYkj/mxzgNH86v7U=
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
```

Compare the output with the table below:

| Platform | Expected Hash |
|----------|---------------|
| **x86_64-linux-gnu** | `sha256-gDsBq6uy7n+sxJ2S88Rvkwr9A+VHHVzEFTJxwbE1CwI=` |
| **aarch64-linux-gnu** | `sha256-oBqXMcKfI8vbCSwd91oGLX/um5xUDfQlO//KLvPhKrQ=` |
| **arm-linux-gnueabihf** | `sha256-5kk9JcdADNB9MXAPqbTCiGwd5Fae2/8A5szisxLIR2I=` |
| **riscv64-linux-gnu** | `sha256-awnROl2xMvLP+Z5sK/M/gcSoMiOaQceBEX+cFkGLkvo=` |
| **win64** | `sha256-8DIqJIf9hIdEjhpBAZcmdCFVwczY80J3puh2mSIydW4=` |
| **x86_64-apple-darwin** | `sha256-QgIz/J9h3G+nBES/lRTSIOfAW25aMAq/3veQj9jKw0U=` |
| **arm64-apple-darwin** | `sha256-4KV87Dnk2rG01Q4Hnt1tb8VjGVWf45nHE+gB1zWbhZ0=` |

**Matching hash = verified build.** Hashes change only when source code, dependencies, or build configuration changes.

## Troubleshooting

### Hash Doesn't Match

If your hash doesn't match the expected value:
- Ensure you're on the same git commit/tag
- Check your Nix version: `nix --version`
- Try a clean build: `nix store gc --max 0` then rebuild (takes 10-20 min)

### Common Issues

| Issue | Solution |
|-------|----------|
| **"experimental-features not enabled"** | Add `experimental-features = nix-command flakes` to `~/.config/nix/nix.conf` and restart Nix daemon |
| **Slow first build** | Normal - ~500 packages built (10-20 min). Subsequent builds are cached (1-2 min) |
| **Out of disk space** | Run `nix store gc` for cleanup or `nix store gc --max 0` for full cleanup (~36GB freed) |
| **macOS cross-arch build slow** | Normal on first build - system compiles dependencies for target architecture |
| **Windows build fails from macOS** | Not supported - use Linux to build Windows binaries |

### Cleaning Build Cache

```bash
# Light cleanup (removes unreferenced packages)
nix store gc

# Full cleanup (removes everything, frees ~36GB)
# Warning: Next build will take 10-20 minutes
nix store gc --max 0
```

Use full cleanup when:
- Testing reproducibility with a clean slate
- Freeing disk space
- Resolving cache issues

## Additional Resources

- [Reproducible Builds Project](https://reproducible-builds.org/)
- [Nix Documentation](https://nixos.org/manual/nix/stable/)
