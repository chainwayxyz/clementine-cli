#!/usr/bin/env bash
#
# Helper script to update git dependency hashes in flake.nix
#
# This script attempts to build the project, captures hash mismatch errors,
# and helps you update the flake.nix with the correct hashes.
#
# Usage:
#   ./reproducible/update-hashes.sh [PLATFORM]
#
# Examples:
#   ./reproducible/update-hashes.sh                    # Auto-detect current platform
#   ./reproducible/update-hashes.sh x86_64-linux-gnu   # Specific platform
#   ./reproducible/update-hashes.sh win64              # Windows cross-compile
#

set -e

FLAKE_NIX="flake.nix"
REPO_ROOT="$(git rev-parse --show-toplevel)"

cd "$REPO_ROOT"

# Detect current platform if not specified
detect_platform() {
    local os=$(uname -s)
    local arch=$(uname -m)

    case "$os" in
        Linux)
            case "$arch" in
                x86_64) echo "x86_64-linux-gnu" ;;
                aarch64) echo "aarch64-linux-gnu" ;;
                armv7l) echo "arm-linux-gnueabihf" ;;
                riscv64) echo "riscv64-linux-gnu" ;;
                *) echo "x86_64-linux-gnu" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "arm64-apple-darwin" ;;
                *) echo "x86_64-apple-darwin" ;;
            esac
            ;;
        *)
            echo "x86_64-linux-gnu"
            ;;
    esac
}

# Get platform from argument or auto-detect
PLATFORM="${1:-$(detect_platform)}"

echo "==> Using platform: $PLATFORM"
echo "==> Attempting to build to discover required hashes..."
echo ""

# Try to build and capture the output
if ! nix build .#$PLATFORM 2>&1 | tee /tmp/nix-build-output.txt; then
    echo ""
    echo "==> Build failed as expected (if this is the first build)"
    echo ""

    # Extract hash information from the error output
    if grep -q "hash mismatch" /tmp/nix-build-output.txt; then
        echo "==> Found hash mismatches. Here are the correct hashes:"
        echo ""

        # Extract package names and hashes
        grep -A 1 "hash mismatch" /tmp/nix-build-output.txt | \
            grep -E "(specified|got):" | \
            sed 's/^[[:space:]]*//' || true

        echo ""
        echo "==> Please update the outputHashes section in flake.nix with these values."
        echo ""
        echo "The outputHashes section should look like:"
        echo ""
        echo "  outputHashes = {"
        echo "    \"bitcoincore-rpc-0.18.0\" = \"sha256-XXXXX...\";"
        echo "    \"secp256k1-0.31.0\" = \"sha256-YYYYY...\";"
        echo "  };"
        echo ""
    else
        echo "==> No hash mismatches found. The error might be something else."
        echo "==> Check /tmp/nix-build-output.txt for details."
    fi

    exit 1
else
    echo ""
    echo "==> Build succeeded! Your hashes are correct."
    echo ""
    nix hash path ./result
    exit 0
fi
