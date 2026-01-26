# Clementine CLI Reproducible Builds

This guide shows how to build Clementine CLI reproducibly using [Nix](https://nixos.org/) and verify your build matches the official release.

Reproducible builds ensure bit-for-bit identical binaries, allowing you to verify that published binaries match the source code exactly.

## Platform Support

**4 platforms** are fully supported and reproducible:

| Platform | Architecture |
|----------|--------------|
| Linux | x86_64, ARM64 |
| Windows | x86_64 |
| macOS | Apple Silicon |

### What You Can Build

| Your System | Can Build For |
|-------------|---------------|
| **Linux (x86_64)** | Linux x86_64, Windows |
| **Linux (ARM64)** | Linux ARM64 |
| **macOS (Apple Silicon)** | macOS Apple Silicon |

> [!NOTE]
> Cross-compilation is supported for Windows from Linux x86_64. For reproducibility, Linux ARM64 and macOS Apple Silicon must be built on their native architecture; cross-compilation is not supported for these.

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
nix build .#linux-x86_64          # Linux x86_64
nix build .#linux-aarch64         # Linux ARM64
nix build .#windows-x86_64        # Windows 64-bit
nix build .#darwin-aarch64        # macOS Apple Silicon

# Binary location:
ls -la ./result/bin/
```

> [!NOTE]
> First build? Expect 10-20 minutes as Nix builds ~500 dependencies. Subsequent builds take 1-2 minutes.

## Supported Platforms

### Working Platforms

Build commands for all working platforms:

```bash
# Linux
nix build .#linux-x86_64            # Intel/AMD 64-bit
nix build .#linux-aarch64           # ARM 64-bit

# macOS (Apple Silicon)
nix build .#darwin-aarch64          # Apple Silicon

# Windows (cross-compile from Linux)
nix build .#windows-x86_64          # 64-bit

# Default (current platform)
nix build
```

**Binary location:** `./result/bin/clementine-cli` (or `.exe` for Windows)

## Platform-Specific Build Instructions

### From Linux

Linux can build for its native architecture and Windows (from x86_64):

```bash
# Linux builds (on x86_64)
nix build .#linux-x86_64            # Intel/AMD 64-bit

# Cross-compile to Windows (from x86_64 Linux)
nix build .#windows-x86_64          # 64-bit

# Linux ARM64 (on ARM64 system)
nix build .#linux-aarch64           # ARM 64-bit
```

### From macOS

Apple Silicon Macs can build for their native platform. macOS Intel builds are not supported by the current flake targets.

```bash
# Check your Mac architecture
uname -m  # "x86_64" = Intel, "arm64" = Apple Silicon

# Build for your current Mac architecture
nix build                           # Automatically selects your architecture

# Apple Silicon Macs can build:
nix build .#darwin-aarch64          # macOS Apple Silicon
```

> [!NOTE]
> macOS Intel builds are not supported by the current Nix flake targets.

> [!NOTE]
> We assume that `/usr/lib/libiconv.2.dylib` is
> present on target macOS systems; if it is not available or you experience
> issues related to this adjustment, please open an issue on this repository so
> the maintainers can assist.

> [!NOTE]
> For Linux and Windows builds, use a Linux system.

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
nix build .#linux-x86_64
HASH1=$(nix hash path ./result)

# Force rebuild (useful for debugging)
nix build .#linux-x86_64 --rebuild
HASH2=$(nix hash path ./result)

# Should match
echo "Build 1: $HASH1"
echo "Build 2: $HASH2"
```

> [!TIP]
> Use `--rebuild` flag to force a rebuild even if the output already exists in the store. This is helpful for debugging and testing reproducibility without clearing the result directory.

### Full Verification (recommended for release verification)

Test with a completely clean Nix store to ensure no cached artifacts affect the build:

```bash
# First clean build
nix build .#linux-x86_64
HASH1=$(nix hash path ./result)

# Clean everything and rebuild
rm -rf result
nix store gc --max 0  # Removes all cached dependencies (~36GB)
nix build .#linux-x86_64  # Takes 10-20 minutes
HASH2=$(nix hash path ./result)

# Verify reproducibility
[ "$HASH1" = "$HASH2" ] && echo " Reproducible!" || echo "Not reproducible"
```

### Cross-Machine Verification

To verify your build matches another developer's:

1. **Ensure identical source**: Both machines must be on the same git commit/tag
2. **Build the binary**: Run `nix build .#<platform>`
3. **Compare hashes**: Both should produce identical `nix hash path ./result` and `sha256sum ./result/bin/clementine-cli` output.

If hashes don't match, verify:
- Same git commit/tag: `git rev-parse HEAD`
- Same Nix version: `nix --version` (recommended: 2.32.1)
- Clean build if needed: `nix store gc --max 0` before rebuilding

> [!NOTE]
> Hashes change when source code, dependencies (Cargo.lock), or build configuration (flake.nix) are modified. Always compare builds from the exact same commit.

## Verifying Downloaded Binaries

If you downloaded a pre-built binary from GitHub Releases, you can verify it matches the source code by building it yourself and comparing hashes.

### Step 1: Check the Release Tag

```bash
# Find the release version from the binary filename
# Example: clementine-cli-v0.1.0-linux-x86_64
# The tag is: v0.1.0

# Clone the repository and checkout the release tag
git clone https://github.com/chainwayxyz/clementine-cli.git
cd clementine-cli
git checkout v0.1.0  # Use the version from your downloaded binary
```

### Step 2: Build the Binary

```bash
# Build for the same platform as your downloaded binary
nix build .#linux-x86_64          # For Linux x86_64
nix build .#linux-aarch64         # For Linux ARM64
nix build .#windows-x86_64        # For Windows
nix build .#darwin-aarch64        # For macOS Apple Silicon

# Add --rebuild to force a fresh build (useful for debugging)
nix build .#linux-x86_64 --rebuild
```

### Step 3: Compare Binaries

```bash
# Compare your downloaded binary with the one you just built
sha256sum /path/to/downloaded/clementine-cli
sha256sum ./result/bin/clementine-cli

# Or compare them directly
diff <(sha256sum /path/to/downloaded/clementine-cli) \
     <(sha256sum ./result/bin/clementine-cli)
```

If the SHA256 hashes match, your downloaded binary is bit-for-bit identical to what the source code produces. This proves:
- The binary matches the published source code exactly
- No modifications were made between building and distribution
- The build process is reproducible

> [!TIP]
> If hashes don't match, ensure you're using the exact same release tag and building for the correct platform.

## Troubleshooting

### Hash Doesn't Match

If your hash doesn't match the expected value:
- Ensure you're on the same git commit/tag
- Check your Nix version: `nix --version`
- Force a rebuild: `nix build .#<platform> --rebuild`
- Try a clean build: `nix store gc --max 0` then rebuild (takes 10-20 min)

### Common Issues

| Issue | Solution |
|-------|----------|
| **"experimental-features not enabled"** | Add `experimental-features = nix-command flakes` to `~/.config/nix/nix.conf` and restart Nix daemon |
| **Slow first build** | Normal - ~500 packages built (10-20 min). Subsequent builds are cached (1-2 min) |
| **Out of disk space** | Run `nix store gc` for cleanup or `nix store gc --max 0` for full cleanup (~36GB freed) |
| **Windows build fails from macOS** | Not supported - use Linux x86_64 to build Windows binaries |
| **Linux ARM64 build fails from macOS** | Not supported - use Linux ARM64 to build Linux ARM64 binaries |

### Cleaning Build Cache

```bash
# Light cleanup (removes unreferenced packages)
nix store gc

# Full cleanup (removes everything, frees ~36GB)
nix store gc --max 0
```

> [!WARNING]
> Next build after full cleanup will take 10-20 minutes.

Use full cleanup when:
- Testing reproducibility with a clean slate
- Freeing disk space
- Resolving cache issues

## Additional Resources

- [Reproducible Builds Project](https://reproducible-builds.org/)
- [Nix Documentation](https://nixos.org/manual/nix/stable/)