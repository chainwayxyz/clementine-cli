#!/usr/bin/env bash
set -euo pipefail

IMAGE_TAG=${IMAGE_TAG:-"nixos/nix:2.32.1"}
ARTIFACTS_DIR=${ARTIFACTS_DIR:-"artifacts"}
PROJECT_NAME="clementine-cli"
NIX_CONFIG=${NIX_CONFIG:-"filter-syscalls = false"}

TARGET_MATRIX=(
  "linux-x86_64 x86_64-linux linux/amd64 "
  "windows-x86_64 x86_64-linux linux/amd64 .exe"
  "aarch64-linux-gnu aarch64-linux linux/arm64 "
)

build_target() {
  local attr="$1" system="$2" platform="$3" suffix="$4"

  echo "[build] $attr using $IMAGE_TAG on platform $platform"

  docker run --rm -t \
    --platform "$platform" \
    -v "$PWD:/workspace" \
    -v "nix-store-$attr:/nix" \
    -w /workspace \
    -e "NIX_CONFIG=${NIX_CONFIG}" \
    "$IMAGE_TAG" \
    bash -c "\
      set -euo pipefail; \
      nix --extra-experimental-features 'nix-command flakes' \
          --accept-flake-config \
          build .#$attr --system $system --print-out-paths | tail -n1 > /tmp/outpath; \
      outPath=\$(cat /tmp/outpath); \
      binSrc="\${outPath}/bin/${PROJECT_NAME}${suffix}"; \
      destDir="/workspace/${ARTIFACTS_DIR}/${attr}"; \
      mkdir -p "\${destDir}"; \
      cp "\${binSrc}" "\${destDir}/${PROJECT_NAME}-${attr}${suffix}"; \
      chmod 0555 "\${destDir}/${PROJECT_NAME}-${attr}${suffix}"; \
      echo \"[done] $attr -> ${ARTIFACTS_DIR}/${attr}/${PROJECT_NAME}-${attr}${suffix}\" \
    "
}

for entry in "${TARGET_MATRIX[@]}"; do
  IFS=' ' read -r attr system platform suffix <<<"$entry"
  build_target "$attr" "$system" "$platform" "$suffix"
  echo
done

echo "All requested builds finished. Darwin binaries require a macOS builder and are not built here."
