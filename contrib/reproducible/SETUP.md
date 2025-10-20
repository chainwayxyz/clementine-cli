# Quick Setup Guide for Reproducible Builds

This is a quick reference for getting started with reproducible builds using Nix.

## Prerequisites Checklist

- [ ] Nix installed (see main README.md)
- [ ] Nix flakes enabled in `~/.config/nix/nix.conf`
- [ ] Nix daemon restarted after configuration

## Quick Start

### 1. First Build (to get dependency hashes)

```bash
# From repository root
./contrib/reproducible/update-hashes.sh
```

This will fail but show you the correct hashes.

### 2. Update flake.nix

Open `flake.nix` and update the `outputHashes` section with the values from step 1.

### 3. Build Successfully

```bash
# For Linux x86_64
nix build .#packages.x86_64-linux.default

# For macOS (if on macOS)
nix build .#packages.aarch64-darwin.default  # Apple Silicon
nix build .#packages.x86_64-darwin.default   # Intel

# For Windows (from Linux only)
nix build .#packages.x86_64-windows.default
```

### 4. Find Your Binary

```bash
ls -la result/bin/
```

## Verifying Reproducibility

```bash
# Build and get hash
nix build .#packages.x86_64-linux.default
nix hash path ./result

# Clean and rebuild
rm -rf result
nix build .#packages.x86_64-linux.default
nix hash path ./result

# Hashes should match!
```

## Platform-Specific Notes

### Linux
- Native builds should work out of the box
- Can cross-compile to Windows

### macOS
- Native builds work on macOS
- Cross-compiling FROM Linux to macOS requires Xcode SDK (see main README)

### Windows
- Must cross-compile from Linux
- Uses MinGW toolchain provided by Nix

## Common Issues

### "experimental-features not enabled"
Add to `~/.config/nix/nix.conf`:
```
experimental-features = nix-command flakes
```
Then restart the Nix daemon.

### "hash mismatch"
Run `./contrib/reproducible/update-hashes.sh` and update the hashes in `flake.nix`.

### Build fails with dependency errors
Try clearing the Nix cache:
```bash
nix store gc
```

## Next Steps

Once you have successful builds:
1. Document the output hashes for your team
2. Set up CI/CD integration (see main README)
3. Create release process documentation

## Learn More

See the detailed [README.md](./README.md) for:
- Complete installation instructions
- macOS SDK setup for cross-compilation
- Release process guidelines
- Advanced troubleshooting
