{
  description = "Clementine CLI – Reproducible cross build for x86_64-pc-windows on x86_64-linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachSystem
      [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ]
      (buildSystem:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { system = buildSystem; inherit overlays; };

          staticPkgs = pkgs.pkgsStatic;

          rustVersion = "1.89.0";

          srcFiltered = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let base = baseNameOf path; in
              !(base == ".git"
                || base == ".github"
                || base == "docs"
                || base == "README.md"
                || base == "artifacts"
                || base == "result"
                || base == "scripts"
                || base == "target");
          };

          targets = {
            windows-x86_64 = {
              crossSystemConfig = "x86_64-w64-mingw32";
              rustTarget = "x86_64-pc-windows-gnu";
              buildOn = [ "aarch64-linux" "x86_64-linux" ];
              useStaticToolchain = false;
            };

            linux-x86_64 = {
              crossSystemConfig = null;
              rustTarget = "x86_64-unknown-linux-musl";
              buildOn = [ "x86_64-linux" ];
              useStaticToolchain = true;
            };

            linux-aarch64 = {
              crossSystemConfig = null;
              rustTarget = "aarch64-unknown-linux-musl";
              buildOn = [ "aarch64-linux" ];
              useStaticToolchain = true;
            };

            darwin-x86_64 = {
              crossSystemConfig = null;
              rustTarget = "x86_64-apple-darwin";
              buildOn = [ "x86_64-darwin" ];
              useStaticToolchain = false;
            };

            darwin-aarch64 = {
              crossSystemConfig = null;
              rustTarget = "aarch64-apple-darwin";
              buildOn = [ "aarch64-darwin" ];
              useStaticToolchain = false;
            };
          };

          mkReproEnv = {
            SOURCE_DATE_EPOCH = "1";
            CARGO_INCREMENTAL = "0";
            ZERO_AR_DATE = "1";
          };

          mkTargetRUSTFLAGS = { targetTriple }:
            let
              isTargetDarwin  = pkgs.lib.hasInfix "apple-darwin" targetTriple;
              isTargetWindows = pkgs.lib.hasInfix "pc-windows-gnu" targetTriple;

              common = [
                "-C" "codegen-units=1"
                "-C" "metadata=clementine-repro"
                "-C" "debuginfo=0"
                "-C" "lto=off"
                "-C" "embed-bitcode=no"
                "--remap-path-prefix=${srcFiltered}=/src"
              ];

              staticCommon = [
                "-C" "target-feature=+crt-static"
                "-C" "link-arg=-static"
                "-C" "link-arg=-Wl,--sort-common"
                "-C" "link-arg=-Wl,--build-id=none"
                "-C" "link-arg=-Wl,-s"
              ];

              linuxOnly = [
                "-C" "link-arg=-Wl,--sort-section=name"
              ];

              windowsOnly = [
                "-C" "link-arg=-Wl,--no-insert-timestamp"
                "-C" "link-arg=-Wl,--sort-section=name"
              ];

              darwinOnly = [ ];

              flags =
                common
                ++ (pkgs.lib.optionals (!isTargetDarwin && !isTargetWindows) (staticCommon ++ linuxOnly))
                ++ (pkgs.lib.optionals isTargetWindows (staticCommon ++ windowsOnly))
                ++ (pkgs.lib.optionals isTargetDarwin darwinOnly);
            in
              pkgs.lib.concatStringsSep " " flags;

          mkTargetPackage = spec:
            let
              targetTriple   = spec.rustTarget;
              rustTargetEnv  = builtins.replaceStrings ["-"] ["_"] targetTriple;
              upperTargetEnv = pkgs.lib.toUpper rustTargetEnv;

              isTargetDarwin = pkgs.lib.hasInfix "apple-darwin" targetTriple;
              isTargetMusl   = pkgs.lib.hasInfix "unknown-linux-musl" targetTriple;

              buildPkgs = pkgs;

              rust = buildPkgs.rust-bin.stable.${rustVersion}.default.override {
                targets = [ targetTriple ];
              };

              rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

              isHostDarwin = buildPkgs.stdenv.isDarwin;
              rustFlags = mkTargetRUSTFLAGS { inherit targetTriple; };

              muslCC = staticPkgs.stdenv.cc;

              staticToolchainEnv =
                pkgs.lib.optionalAttrs (spec.useStaticToolchain && isTargetMusl) {
                  "CC_${rustTargetEnv}" =
                    "${muslCC}/bin/${muslCC.targetPrefix}cc";
                  "AR_${rustTargetEnv}" =
                    "${muslCC}/bin/${muslCC.targetPrefix}ar";
                  "CARGO_TARGET_${upperTargetEnv}_LINKER" =
                    "${muslCC}/bin/${muslCC.targetPrefix}cc";
                };

              staticNativeBuildInputs =
                pkgs.lib.optionals (spec.useStaticToolchain && isTargetMusl) [
                  staticPkgs.sqlite
                ];
            in
            rustPlatform.buildRustPackage rec {
              pname = "clementine-cli";
              version = "0.1.0";
              src = srcFiltered;

              nativeBuildInputs =
                [ buildPkgs.pkg-config ]
                ++ pkgs.lib.optionals isHostDarwin [ buildPkgs.libiconv ];

              buildInputs = staticNativeBuildInputs;

              cargoBuildFlags = [ 
                "--target=${targetTriple}" 
                "--locked"
              ];

              env =
                mkReproEnv
                // staticToolchainEnv
                // {
                  "CARGO_TARGET_${upperTargetEnv}_RUSTFLAGS" = rustFlags;
                };

              preBuild = pkgs.lib.optionalString isTargetDarwin ''
                export CARGO_TARGET_${upperTargetEnv}_RUSTFLAGS="$CARGO_TARGET_${upperTargetEnv}_RUSTFLAGS \
                  -C link-arg=-Wl,-oso_prefix,$(realpath $NIX_BUILD_TOP)/ \
                  --remap-path-prefix=$NIX_BUILD_TOP=/build"
                export NIX_CFLAGS_COMPILE="$NIX_CFLAGS_COMPILE -fdebug-prefix-map=$NIX_BUILD_TOP=/build -fdebug-prefix-map=${src}=/src"
                echo "Applied Darwin-specific reproducibility flags to ${upperTargetEnv}"
              '';

              cargoLock = {
                lockFile = ./Cargo.lock;
                outputHashes = {
                  "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
                  "secp256k1-0.31.0" = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
                };
              };

              installPhase = ''
                mkdir -p $out/bin
                cp target/${targetTriple}/release/clementine-cli $out/bin/
                chmod 555 $out/bin/clementine-cli
              '';

              postFixup = pkgs.lib.optionalString (isHostDarwin && isTargetDarwin) ''
                bin="$out/bin/clementine-cli"

                chmod +w "$bin"

                otool="${pkgs.darwin.cctools}/bin/otool"
                install_name_tool="${pkgs.darwin.cctools}/bin/install_name_tool"
                codesign_allocate="${pkgs.darwin.binutils.bintools}/bin/codesign_allocate"
                codesign="${pkgs.darwin.sigtool}/bin/codesign"

                LIBICONV_PATH="$($otool -L "$bin" | awk '/libiconv\.2\.dylib/{print $1; exit}')"
                if [ -n "$LIBICONV_PATH" ]; then
                  $install_name_tool \
                    -change "$LIBICONV_PATH" /usr/lib/libiconv.2.dylib \
                    "$bin"
                fi

                CODESIGN_ALLOCATE="$codesign_allocate" \
                  "$codesign" -f -s - "$bin"

                chmod 555 "$bin"
              '';

              doCheck = false;
              auditable = false;
              dontStrip = true;
            };

        in {
          packages =
            let
              windows-x86_64 =
                if builtins.elem buildSystem targets.windows-x86_64.buildOn then
                  let
                    targetTriple = "x86_64-pc-windows-gnu";

                    crossPkgs = import nixpkgs {
                      system = buildSystem;
                      crossSystem = { config = "x86_64-w64-mingw32"; };
                      inherit overlays;
                    };

                    buildPkgs = crossPkgs.buildPackages;

                    rust = buildPkgs.rust-bin.stable.${rustVersion}.default.override {
                      targets = [ targetTriple ];
                    };

                    rustPlatform = crossPkgs.makeRustPlatform {
                      cargo = rust;
                      rustc  = rust;
                    };

                    rustTargetEnv = builtins.replaceStrings ["-"] ["_"] targetTriple;
                  in
                  rustPlatform.buildRustPackage rec {
                    pname = "clementine-cli";
                    version = "0.1.0";
                    src = srcFiltered;

                    nativeBuildInputs =
                      [ buildPkgs.pkg-config ]
                      ++ pkgs.lib.optionals buildPkgs.stdenv.isDarwin [ buildPkgs.libiconv ];

                    cargoBuildFlags = [ 
                      "--target=${targetTriple}" 
                      "--locked"
                    ];

                    env = {
                      "CC_${rustTargetEnv}" =
                          "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}gcc";
                      "AR_${rustTargetEnv}" =
                          "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}ar";

                      "CARGO_TARGET_${pkgs.lib.toUpper rustTargetEnv}_LINKER" =
                        "${crossPkgs.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";

                      "CARGO_TARGET_${pkgs.lib.toUpper rustTargetEnv}_RUSTFLAGS" =
                       "-C target-feature=+crt-static \
                        -C link-arg=-static \
                        -C link-arg=-Wl,--no-insert-timestamp \
                        -C link-arg=-Wl,--sort-section=name \
                        -C link-arg=-Wl,--sort-common \
                        -C link-arg=-Wl,--build-id=none \
                        -C link-arg=-Wl,-s \
                        -C codegen-units=1 \
                        -C metadata=clementine-repro \
                        -C debuginfo=0 \
                        -C lto=off \
                        -C embed-bitcode=no \
                        -C target-cpu=x86-64 \
                        --remap-path-prefix=${srcFiltered}=/src";
                      SOURCE_DATE_EPOCH = "1";
                      CARGO_INCREMENTAL = "0";
                      ZERO_AR_DATE = "1";
                    };

                    cargoLock = {
                      lockFile = ./Cargo.lock;
                      outputHashes = {
                        "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
                        "secp256k1-0.31.0" = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
                      };
                    };

                    installPhase = ''
                      mkdir -p $out/bin
                      cp target/${targetTriple}/release/clementine-cli.exe $out/bin/
                      chmod 555 $out/bin/clementine-cli.exe
                    '';

                    doCheck = false;
                    auditable = false;
                    dontStrip = true;
                  }
                else
                  null;

              linux-x86_64 =
                if builtins.elem buildSystem targets.linux-x86_64.buildOn
                then mkTargetPackage targets.linux-x86_64
                else null;

              linux-aarch64 =
                if builtins.elem buildSystem targets.linux-aarch64.buildOn
                then mkTargetPackage targets.linux-aarch64
                else null;

              darwin-x86_64 =
                if builtins.elem buildSystem targets.darwin-x86_64.buildOn
                then mkTargetPackage targets.darwin-x86_64
                else null;

              darwin-aarch64 =
                if builtins.elem buildSystem targets.darwin-aarch64.buildOn
                then mkTargetPackage targets.darwin-aarch64
                else null;

              cleaned =
                pkgs.lib.filterAttrs (_: v: v != null) {
                  inherit
                    windows-x86_64
                    linux-x86_64
                    linux-aarch64
                    darwin-x86_64
                    darwin-aarch64;
                };

              defaultPkg =
                if buildSystem == "x86_64-linux" then linux-x86_64 else
                if buildSystem == "aarch64-linux" then linux-aarch64 else
                if buildSystem == "x86_64-darwin" then darwin-x86_64 else
                if buildSystem == "aarch64-darwin" then darwin-aarch64 else
                null;
            in
              cleaned // (pkgs.lib.optionalAttrs (defaultPkg != null) {
                default = defaultPkg;
              });
        }
      );
}
