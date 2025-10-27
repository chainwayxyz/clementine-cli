#!/usr/bin/env bash
#
# Update all documented hashes in reproducible-builds.md
#
# This script builds all platforms and updates the Expected Hashes table
# in docs/reproducible-builds.md with the current build hashes.
#
# Usage:
#   ./reproducible/update-documented-hashes.sh
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

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Update Documented Hashes${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Determine which platforms we can build on this system
PLATFORMS=()
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo -e "${GREEN}Detected Linux - building all platforms${NC}"
    PLATFORMS=("x86_64-linux-gnu" "aarch64-linux-gnu" "arm-linux-gnueabihf" "riscv64-linux-gnu" "win64" "x86_64-apple-darwin" "arm64-apple-darwin")
elif [[ "$OSTYPE" == "darwin"* ]]; then
    echo -e "${GREEN}Detected macOS - building macOS platforms only${NC}"
    PLATFORMS=("x86_64-apple-darwin" "arm64-apple-darwin")
    echo -e "${YELLOW}Note: Linux and Windows platforms should be built on Linux${NC}"
else
    echo -e "${RED}Unsupported platform: $OSTYPE${NC}"
    exit 1
fi

echo ""

# Build each platform and collect hashes
declare -A HASHES

for platform in "${PLATFORMS[@]}"; do
    echo -e "${BLUE}Building ${platform}...${NC}"

    if nix build .#${platform}; then
        HASH=$(nix --extra-experimental-features 'nix-command flakes' hash path ./result)
        HASHES[$platform]=$HASH
        echo -e "${GREEN}✓${NC} ${platform}: ${HASH}"
        rm -rf result
    else
        echo -e "${RED}✗ Failed to build ${platform}${NC}"
        exit 1
    fi
    echo ""
done

# Create updated hash table
echo -e "${BLUE}Generating updated hash table...${NC}"
echo ""

TEMP_FILE=$(mktemp)

cat > "$TEMP_FILE" << 'EOF'
| Platform | Hash |
|----------|------|
EOF

# Add each hash in the correct order
declare -a PLATFORM_ORDER=("x86_64-linux-gnu" "aarch64-linux-gnu" "arm-linux-gnueabihf" "riscv64-linux-gnu" "win64" "x86_64-apple-darwin" "arm64-apple-darwin")

for platform in "${PLATFORM_ORDER[@]}"; do
    if [[ -n "${HASHES[$platform]}" ]]; then
        echo "| **${platform}** | \`${HASHES[$platform]}\` |" >> "$TEMP_FILE"
    fi
done

echo -e "${GREEN}Updated hash table:${NC}"
echo ""
cat "$TEMP_FILE"
echo ""

# Ask user if they want to update the documentation
read -p "Update docs/reproducible-builds.md with these hashes? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    # Update the documentation
    # We'll replace the table between "| Platform | Hash |" and the next blank line or ">"

    # Create a backup
    cp docs/reproducible-builds.md docs/reproducible-builds.md.backup
    echo -e "${GREEN}Created backup: docs/reproducible-builds.md.backup${NC}"

    # Use awk to replace the hash table
    awk -v new_table="$(cat $TEMP_FILE)" '
        /^\| Platform \| Hash \|/ {
            print new_table
            in_table = 1
            next
        }
        in_table && /^$/ {
            in_table = 0
        }
        in_table && /^>/ {
            in_table = 0
        }
        !in_table || !/^\|/ {
            print
        }
    ' docs/reproducible-builds.md.backup > docs/reproducible-builds.md

    echo -e "${GREEN}✓ Updated docs/reproducible-builds.md${NC}"
    echo ""
    echo -e "${YELLOW}Please review the changes with:${NC}"
    echo -e "  git diff docs/reproducible-builds.md"
else
    echo "Update cancelled."
fi

rm "$TEMP_FILE"
