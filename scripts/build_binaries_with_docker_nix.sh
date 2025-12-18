#!/usr/bin/env bash
set -euo pipefail

IMAGE_TAG="${IMAGE_TAG:-nixos/nix:2.32.1}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts}"
PROJECT_NAME="${PROJECT_NAME:-clementine-cli}"
NIX_CONFIG="${NIX_CONFIG:-filter-syscalls = false}"

TARGET_MATRIX=(
  "linux-x86_64 linux/amd64"
  "windows-x86_64 linux/amd64 .exe"
  "aarch64-linux-gnu linux/arm64"
)

build_target() {
  local attr="$1"
  local platform="$2"
  local suffix="${3-}"

  echo "[build] $attr using $IMAGE_TAG on platform $platform"

  docker run --rm -i --privileged \
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

nix --extra-experimental-features 'nix-command flakes' show-config | grep -E 'sandbox|build-use-sandbox'

outPath="$(
  nix --extra-experimental-features 'nix-command flakes' \
      --accept-flake-config \
      build ".#${ATTR}" --option sandbox true --print-out-paths | tail -n1
)"

nix --extra-experimental-features 'nix-command flakes' show-config | grep -E 'sandbox|build-use-sandbox'

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

echo "All requested builds finished. Darwin binaries require a macOS builder and are not built here."
