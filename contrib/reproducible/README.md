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

All builds are performed from the repository root. The Nix flake configuration supports cross-compilation from macOS or Linux to multiple target platforms.

### Quick Reference

**All Supported Build Commands:**

```bash
# Native build (your current system)
nix build

# Linux targets
nix build .#x86_64-linux        # Linux Intel/AMD 64-bit
nix build .#aarch64-linux       # Linux ARM 64-bit

# macOS targets
nix build .#x86_64-darwin       # macOS Intel
nix build .#aarch64-darwin      # macOS Apple Silicon (M1/M2/M3)

# Windows target
nix build .#x86_64-windows      # Windows 64-bit (see platform notes below)
```

The built binary will be in `./result/bin/clementine-cli` (or `.exe` for Windows).

### Cross-Compilation Overview

The flake supports building binaries for:
- **Linux**: x86_64, ARM64 (aarch64)
- **macOS**: Intel (x86_64), Apple Silicon (aarch64)
- **Windows**: x86_64

### Cross-Compilation Matrix

This table shows which targets can be built from which build systems:

| Build System → Target | Linux x86_64 | Linux ARM64 | macOS Intel (x86_64) | macOS Apple Silicon (M1/M2/M3) | Windows x86_64 |
|----------------------|--------------|-------------|----------------------|-------------------------------|----------------|
| **macOS Intel**      | ✅ Works     | ✅ Works    | ✅ Works (native)    | ✅ Works                      | ⚠️ Experimental¹ |
| **macOS Apple Silicon** | ✅ Works  | ✅ Works    | ✅ Works             | ✅ Works (native)             | ⚠️ Experimental¹ |
| **Linux x86_64**     | ✅ Works (native) | ✅ Works | ✅ Works²           | ✅ Works²                     | ✅ Works       |
| **Linux ARM64**      | ✅ Works     | ✅ Works (native) | ✅ Works²      | ✅ Works²                     | ✅ Works       |

> [!WARNING]
> Windows from macOS: Cross-compilation to Windows from macOS (both Intel and Apple Silicon) is experimental and may encounter issues with MinGW toolchain availability. For reliable Windows builds, use a Linux build system or CI/CD.

> [!NOTE]
> macOS from Linux: Requires the macOS SDK (Xcode 12.2) to be manually installed. See [Prerequisites for macOS Cross-Compilation](#prerequisites-for-macos-cross-compilation-from-linux) below.

### Testing Your Build Environment

To verify which targets your current system can build, you can test with a simple command:

```bash
# Check your current system
nix eval --raw .#currentSystem
echo ""

# Try building for your native platform (should always work)
nix build

# Test cross-compilation (choose based on your build system)
# These examples test without actually completing the full build
nix flake show
```

The `nix flake show` command will display all available build targets without actually building them.

### Building from macOS

> [!NOTE]
> These instructions work for both Intel Macs (x86_64) and Apple Silicon Macs (M1/M2/M3/M4 - aarch64). Your Mac can cross-compile to any target regardless of which chip it has.

To check which architecture your Mac is:
```bash
uname -m
# x86_64 = Intel Mac
# arm64 = Apple Silicon Mac (M1/M2/M3/M4)
```

#### For Linux (x86_64)

```bash
# Cross-compile from macOS to Linux x86_64
nix build .#x86_64-linux
```

The binary will be in `./result/bin/clementine-cli`.

#### For Linux (ARM64)

```bash
# Cross-compile from macOS to Linux ARM64
nix build .#aarch64-linux
```

#### For macOS (other architecture)

You can cross-compile between macOS architectures:

```bash
# Build for Intel Macs (x86_64)
nix build .#x86_64-darwin

# Build for Apple Silicon Macs (M1/M2/M3/M4 - aarch64)
nix build .#aarch64-darwin

# Build for your current Mac architecture (native)
# This automatically detects whether you have Intel or Apple Silicon
nix build
```

> [!TIP]
> - If you have an M1/M2/M3 Mac and run `nix build`, it builds for `aarch64-darwin`
> - If you have an Intel Mac and run `nix build`, it builds for `x86_64-darwin`
> - Either type of Mac can build for the other using the explicit commands above

#### For Windows (x86_64)

> [!WARNING]
> Windows cross-compilation from macOS is experimental and may fail due to MinGW toolchain limitations. For production Windows builds, use Linux (see below) or CI/CD.

```bash
# Cross-compile from macOS to Windows x86_64 (experimental)
nix build .#x86_64-windows
```

If successful, the binary will be `./result/bin/clementine-cli.exe`.

> [!TIP]
> If the Windows build fails from macOS, you can:
> - Build from a Linux system instead (see [Building from Linux](#building-from-linux))
> - Use CI/CD to build Windows binaries automatically
> - Build all other platforms from macOS and only build Windows separately

### Building from Linux

#### For Linux (different architecture)

```bash
# On x86_64 Linux, build for ARM64
nix build .#aarch64-linux

# On ARM64 Linux, build for x86_64
nix build .#x86_64-linux

# Build for your current architecture (native)
nix build
```

#### For macOS

> [!NOTE]
> Cross-compiling from Linux to macOS requires the macOS SDK.

##### Prerequisites for macOS Cross-Compilation (from Linux)

To build macOS binaries from Linux, you need to obtain the macOS SDK. The Xcode 12.2 version is required (`Xcode_12.2.xip`).

> [!IMPORTANT]
> You need to download this from Apple's website. An Apple ID is required (you can create one for free).

> [!CAUTION]
> The Xcode SDK archive itself cannot be redistributed (it's covered by Apple's license agreement). However, binaries built using the SDK can be freely distributed. Each developer performing macOS cross-compilation must download their own copy of the SDK from Apple.

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

##### Building for macOS (from Linux)

After setting up the macOS SDK:

**Apple Silicon**:
```bash
nix build .#aarch64-darwin
```

**Intel**:
```bash
nix build .#x86_64-darwin
```

#### For Windows (x86_64)

```bash
# Cross-compile from Linux to Windows x86_64
nix build .#x86_64-windows
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

   > [!TIP]
   > Recommended approach: Build 4 platforms from macOS (or Linux), and build Windows from Linux or CI/CD.

   From macOS:
   ```bash
   # Linux x86_64
   nix build .#x86_64-linux
   cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-x86_64

   # Linux ARM64
   nix build .#aarch64-linux
   cp result/bin/clementine-cli clementine-cli-v0.1.0-linux-aarch64

   # macOS Apple Silicon
   nix build .#aarch64-darwin
   cp result/bin/clementine-cli clementine-cli-v0.1.0-macos-aarch64

   # macOS Intel
   nix build .#x86_64-darwin
   cp result/bin/clementine-cli clementine-cli-v0.1.0-macos-x86_64
   ```

   From Linux (for Windows):
   ```bash
   # Windows (more reliable from Linux)
   nix build .#x86_64-windows
   cp result/bin/clementine-cli.exe clementine-cli-v0.1.0-windows-x86_64.exe
   ```

   Or from macOS (experimental):
   ```bash
   # Windows (may require troubleshooting)
   nix build .#x86_64-windows
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

#### Windows from macOS fails

> [!WARNING]
> This is a known limitation due to MinGW toolchain availability on macOS.

Solutions:
- Build Windows binaries from a Linux system (native or VM)
- Use GitHub Actions or other CI/CD to build Windows binaries
- Try updating your Nix packages: `nix flake update`

#### Windows from Linux fails

Check the following:
- Ensure you're using a recent version of nixpkgs with MinGW support
- Check that the build logs show the correct target triple: `x86_64-pc-windows-gnu`

#### macOS from Linux fails

Verify your SDK setup:
- Verify you have installed the macOS SDK (Xcode 12.2) correctly
- Check that the SDK is in the Nix store: `nix-store --query --references $(nix-store -q --deriver $(which nix))`

### Cache Issues

If you encounter caching issues, you can clear the Nix store cache:

```bash
nix store gc
```

## Platform-Specific Best Practices

### Building All Platforms for a Release

> [!NOTE]
> If you're on macOS:
> 1. Build all macOS and Linux targets directly (4 platforms total)
> 2. Either:
>    - Try building Windows and keep if successful, OR
>    - Use a Linux machine/VM for the Windows build, OR
>    - Use CI/CD to build Windows automatically

> [!NOTE]
> If you're on Linux:
> 1. Build all targets directly (all 5 platforms)
> 2. For macOS targets, first install the macOS SDK (one-time setup)

Recommended Multi-Platform Workflow:
```bash
# 1. Test native build first
nix build && ./result/bin/clementine-cli --version

# 2. Build all reliable cross-compilation targets
nix build .#x86_64-linux
nix build .#aarch64-linux
nix build .#x86_64-darwin
nix build .#aarch64-darwin

# 3. Build Windows (adjust based on your build system)
nix build .#x86_64-windows  # May need Linux for reliable builds
```

### Verifying Cross-Compiled Binaries

After cross-compiling, verify the binary is for the correct architecture:

```bash
# For Linux binaries
file ./result/bin/clementine-cli
# Should show: ELF 64-bit LSB executable, x86-64 or ARM aarch64

# For macOS binaries
file ./result/bin/clementine-cli
# Should show: Mach-O 64-bit executable x86_64 or arm64

# For Windows binaries
file ./result/bin/clementine-cli.exe
# Should show: PE32+ executable (console) x86-64
```

## CI/CD Integration

### Future Enhancement: Automated Reproducible Builds

While not yet implemented, integrating Nix builds into CI/CD would provide:

1. **Automated verification** of reproducibility on every PR
2. **Multi-platform builds** in a single workflow
3. **Guaranteed consistency** between development and release builds

### Example GitHub Actions Workflow

Here's a template for future CI/CD integration:

```yaml
name: Reproducible Builds

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v24
        with:
          extra_nix_config: |
            experimental-features = nix-command flakes

      - name: Build for Linux x86_64
        run: nix build .#packages.x86_64-linux.default

      - name: Verify reproducibility
        run: |
          HASH1=$(nix hash path ./result)
          rm -rf result
          nix build .#packages.x86_64-linux.default
          HASH2=$(nix hash path ./result)
          if [ "$HASH1" != "$HASH2" ]; then
            echo "Build is not reproducible!"
            exit 1
          fi
          echo "Build is reproducible: $HASH1"

  build-macos:
    runs-on: macos-latest
    strategy:
      matrix:
        arch: [aarch64-darwin, x86_64-darwin]
    steps:
      - uses: actions/checkout@v4
      - uses: cachix/install-nix-action@v24
        with:
          extra_nix_config: |
            experimental-features = nix-command flakes

      - name: Build for macOS
        run: nix build .#packages.${{ matrix.arch }}.default
```

### Benefits of CI Integration

- **Early detection** of reproducibility issues
- **Platform coverage** testing on every commit
- **Release automation** with verified builds
- **Hash verification** before publishing releases

### Implementation Steps

1. Add the workflow file to `.github/workflows/reproducible-builds.yml`
2. Configure Cachix or another binary cache for faster builds
3. Set up automated attestation/signing for releases
4. Document the CI process in the main README

This ensures that all releases are built using the same deterministic process, providing users with verifiable binaries.

## Learn More

- [Reproducible Builds](https://reproducible-builds.org/) - About reproducible builds
- [Nix Manual](https://nixos.org/manual/nix/stable/) - Complete Nix documentation
- [Nix Flakes](https://nixos.wiki/wiki/Flakes) - About Nix flakes
- [GitHub Actions + Nix](https://github.com/cachix/install-nix-action) - CI integration guide
