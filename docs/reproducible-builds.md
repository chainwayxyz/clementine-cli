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

**First build takes 10-20 minutes.** Subsequent builds are cached and take 1-2 minutes.

## Verify Your Build

After building, verify your binary matches the expected hash:

```bash
nix hash path ./result
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
