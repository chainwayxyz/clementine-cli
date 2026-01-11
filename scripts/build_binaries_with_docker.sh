#!/usr/bin/env bash
set -euo pipefail

IMAGE_TAG="${IMAGE_TAG:-nixos/nix:2.32.1}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts}"
PROJECT_NAME="${PROJECT_NAME:-clementine-cli}"

NIX_CONFIG="${NIX_CONFIG:-extra-experimental-features = nix-command flakes
filter-syscalls = false
sandbox = true}"

TARGET_MATRIX=(
  "linux-x86_64 linux/amd64"
  "windows-x86_64 linux/amd64 .exe"
  "aarch64-linux-gnu linux/arm64"
)

MACOS_TARGET_MATRIX=(
  "darwin-aarch64 aarch64-darwin"
)

build_target() {
  local attr="$1"
  local platform="$2"
  local suffix="${3-}"

  echo "[build] $attr using $IMAGE_TAG on platform $platform"

  docker run --rm -i \
    --privileged \
    --platform "$platform" \
    -v "$PWD:/workspace" \
    -v "nix-store-$attr:/nix" \
    -w /workspace \
    -e "NIX_CONFIG=$NIX_CONFIG" \
    -e "ATTR=$attr" \
    -e "SUFFIX=$suffix" \
    -e "ARTIFACTS_DIR=$ARTIFACTS_DIR" \
    -e "PROJECT_NAME=$PROJECT_NAME" \
    "$IMAGE_TAG" \
    bash -seu <<'EOF'
set -o pipefail

# Ensure /tmp exists and has correct permissions for the sandbox
mkdir -p /tmp && chmod 1777 /tmp

outPath="$(
  nix build "path:/workspace#${ATTR}" --print-out-paths | tail -n1
)"

binSrc="${outPath}/bin/${PROJECT_NAME}${SUFFIX}"
destDir="/workspace/${ARTIFACTS_DIR}/${ATTR}"

mkdir -p "$destDir"
cp "$binSrc" "$destDir/${PROJECT_NAME}${SUFFIX}"

# Skip chmod for Windows targets (based on attr)
if [[ "$ATTR" != windows-* ]]; then
  chmod 0555 "$destDir/${PROJECT_NAME}${SUFFIX}"
fi

echo "[done] ${ATTR} -> ${ARTIFACTS_DIR}/${ATTR}/${PROJECT_NAME}${SUFFIX}"
EOF
}

for entry in "${TARGET_MATRIX[@]}"; do
  read -r attr platform suffix <<<"$entry"
  build_target "$attr" "$platform" "${suffix-}"
  echo
done

# If on macOS, build native macOS binaries without Docker
if [[ "$(uname)" == "Darwin" ]]; then
  for entry in "${MACOS_TARGET_MATRIX[@]}"; do
    read -r attr system <<<"$entry"
    # Safely get current system string
    host_nix_system=$(nix eval --impure --expr 'builtins.currentSystem' | tr -d '"')
    
    if [[ "$host_nix_system" == "$system" ]]; then
      echo "[build] $attr natively on macOS ($system)"
      nix build ".#${attr}"
      outPath=$(nix path-info ".#${attr}" | tail -n1)
      binSrc="${outPath}/bin/${PROJECT_NAME}"
      destDir="${ARTIFACTS_DIR}/${attr}"
      mkdir -p "$destDir"
      rm -rf "$destDir/${PROJECT_NAME}"
      cp "$binSrc" "$destDir/${PROJECT_NAME}"
      chmod 0555 "$destDir/${PROJECT_NAME}"
      echo "[done] $attr -> ${ARTIFACTS_DIR}/${attr}/${PROJECT_NAME}"
      echo
    else
      echo "[skip] $attr: host system ($host_nix_system) does not match target system ($system)"
    fi
  done
fi

echo "All requested builds finished."