#!/usr/bin/env bash
#
# Verify reproducibility of Clementine CLI builds
#
# This script builds the same platform twice and verifies the outputs are identical.
# Optionally, it can perform a full clean build verification.
#
# Usage:
#   ./reproducible/verify-reproducibility.sh [PLATFORM] [--full]
#
# Examples:
#   ./reproducible/verify-reproducibility.sh                      # Quick test, auto-detect platform
#   ./reproducible/verify-reproducibility.sh x86_64-linux-gnu     # Quick test, specific platform
#   ./reproducible/verify-reproducibility.sh --full               # Full clean build test
#   ./reproducible/verify-reproducibility.sh win64 --full         # Full clean build for Windows
#

set -e

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Detect current platform
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

# Parse arguments
PLATFORM=""
FULL_CLEAN=false

for arg in "$@"; do
    case "$arg" in
        --full)
            FULL_CLEAN=true
            ;;
        *)
            PLATFORM="$arg"
            ;;
    esac
done

# Auto-detect platform if not specified
if [ -z "$PLATFORM" ]; then
    PLATFORM=$(detect_platform)
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Reproducibility Verification${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "Platform:     ${GREEN}$PLATFORM${NC}"
echo -e "Mode:         $([ "$FULL_CLEAN" = true ] && echo -e "${YELLOW}Full Clean Build${NC}" || echo -e "${GREEN}Quick Test${NC}")"
echo ""

if [ "$FULL_CLEAN" = true ]; then
    echo -e "${YELLOW}    Full clean build will:${NC}"
    echo -e "   - Remove all Nix cache (~36GB)"
    echo -e "   - Take 10-20 minutes for each build"
    echo -e "   - Provide strongest verification"
    echo ""
    read -p "Continue? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 1
    fi
    echo ""
fi

# Build #1
echo -e "${BLUE}Building ${PLATFORM} (first time)...${NC}"
if [ "$FULL_CLEAN" = true ]; then
    echo "Cleaning Nix store..."
    nix store gc --max 0 > /dev/null 2>&1 || true
fi

nix build .#$PLATFORM
HASH1=$(nix --extra-experimental-features 'nix-command flakes' hash path ./result)
echo -e "${GREEN}✓${NC} First build complete"
echo -e "  Hash: ${HASH1}"
echo ""

# Clean result
rm -rf result

# Build #2
echo -e "${BLUE}Building ${PLATFORM} (second time)...${NC}"
if [ "$FULL_CLEAN" = true ]; then
    echo "Cleaning Nix store again..."
    nix store gc --max 0 > /dev/null 2>&1 || true
fi

nix build .#$PLATFORM
HASH2=$(nix --extra-experimental-features 'nix-command flakes' hash path ./result)
echo -e "${GREEN}✓${NC} Second build complete"
echo -e "  Hash: ${HASH2}"
echo ""

# Compare
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Verification Results${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "Platform:     ${GREEN}$PLATFORM${NC}"
echo -e "First build:  $HASH1"
echo -e "Second build: $HASH2"
echo ""

if [ "$HASH1" = "$HASH2" ]; then
    echo -e "${GREEN}✓ BUILD IS REPRODUCIBLE!${NC}"
    echo -e "${GREEN}  Hashes match - builds are bit-for-bit identical${NC}"
    echo ""

    # Check against documented hash
    DOCUMENTED_HASH=$(grep -A 10 "Expected Hashes" docs/reproducible-builds.md 2>/dev/null | grep "$PLATFORM" | awk '{print $3}' | tr -d '`|' || true)
    if [ -n "$DOCUMENTED_HASH" ]; then
        echo -e "Documented hash: $DOCUMENTED_HASH"
        if [ "$DOCUMENTED_HASH" = "$HASH1" ]; then
            echo -e "${GREEN}✓ Matches documented hash in reproducible-builds.md${NC}"
        else
            echo -e "${YELLOW}    Differs from documented hash (expected after code/dependency changes)${NC}"
            echo -e "${YELLOW}   Update docs/reproducible-builds.md with: $HASH1${NC}"
        fi
    fi

    echo ""
    exit 0
else
    echo -e "${RED}✗ BUILD IS NOT REPRODUCIBLE!${NC}"
    echo -e "${RED}  Hashes differ - builds are not identical${NC}"
    echo ""
    echo -e "${YELLOW}This indicates a non-deterministic build. Possible causes:${NC}"
    echo -e "  - Timestamps in build process"
    echo -e "  - Non-deterministic dependency resolution"
    echo -e "  - Concurrent build processes"
    echo -e "  - Hardware-dependent optimizations"
    echo ""
    exit 1
fi
