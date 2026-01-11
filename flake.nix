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
            };

            linux-x86_64 = {
              crossSystemConfig = "x86_64-unknown-linux-musl";
              rustTarget = "x86_64-unknown-linux-musl";
              buildOn = [ "x86_64-linux" "aarch64-linux" ];
            };

            linux-aarch64 = {
              crossSystemConfig = "aarch64-unknown-linux-musl";
              rustTarget = "aarch64-unknown-linux-musl";
              buildOn = [ "x86_64-linux" "aarch64-linux" ];
            };

            darwin-x86_64 = {
              crossSystemConfig = null;
              rustTarget = "x86_64-apple-darwin";
              buildOn = [ "x86_64-darwin" ];
            };

            darwin-aarch64 = {
              crossSystemConfig = null;
              rustTarget = "aarch64-apple-darwin";
              buildOn = [ "aarch64-darwin" ];
            };
          };

          mkReproEnv = {
            SOURCE_DATE_EPOCH = "1";
            CARGO_INCREMENTAL = "0";
            ZERO_AR_DATE = "1";
          };

          mkTargetRUSTFLAGS = { targetTriple }:
            let
              isTargetDarwin = pkgs.lib.hasInfix "apple-darwin" targetTriple;

              cpuFlag =
                pkgs.lib.optionalString (pkgs.lib.hasInfix "x86_64" targetTriple) "-C target-cpu=x86-64"
              + pkgs.lib.optionalString (pkgs.lib.hasInfix "aarch64" targetTriple) "-C target-cpu=generic";

              common = [
                "-C codegen-units=1"
                "-C metadata=clementine-repro"
                "-C debuginfo=0"
                "-C lto=off"
                "-C embed-bitcode=no"
                "--remap-path-prefix=${srcFiltered}=/src"
                "--remap-path-prefix=$NIX_BUILD_TOP=/build"
              ];

              staticish = [
                "-C target-feature=+crt-static"
                "-C link-arg=-static"
                "-C link-arg=-Wl,--no-insert-timestamp"
                "-C link-arg=-Wl,--sort-section=name"
                "-C link-arg=-Wl,--sort-common"
                "-C link-arg=-Wl,--build-id=none"
                "-C link-arg=-Wl,-s"
              ];

              darwinish = [
                "-C link-arg=-Wl,-no_uuid"
              ];

              flags =
                common
                ++ (pkgs.lib.optionals (!isTargetDarwin) staticish)
                ++ (pkgs.lib.optionals isTargetDarwin darwinish)
                ++ (pkgs.lib.optionals (cpuFlag != "") [ cpuFlag ]);
            in
              pkgs.lib.concatStringsSep " \\\n" flags;

          # Generic builder for non-windows targets (linux musl + darwin)
          mkTargetPackage = spec:
            let
              targetTriple = spec.rustTarget;
              rustTargetEnv = builtins.replaceStrings ["-"] ["_"] targetTriple;
              upperTargetEnv = pkgs.lib.toUpper rustTargetEnv;

              crossPkgs =
                if spec.crossSystemConfig == null
                then pkgs
                else import nixpkgs {
                  system = buildSystem;
                  crossSystem = { config = spec.crossSystemConfig; };
                  inherit overlays;
                };

              buildPkgs =
                if spec.crossSystemConfig == null
                then pkgs
                else crossPkgs.buildPackages;

              rust = buildPkgs.rust-bin.stable.${rustVersion}.default.override {
                targets = [ targetTriple ];
              };

              rustPlatform =
                (if spec.crossSystemConfig == null
                 then pkgs.makeRustPlatform
                 else crossPkgs.makeRustPlatform) {
                  cargo = rust;
                  rustc = rust;
                };

              isHostDarwin = buildPkgs.stdenv.isDarwin;
              rustFlags = mkTargetRUSTFLAGS { inherit targetTriple; };
            in
            rustPlatform.buildRustPackage rec {
              pname = "clementine-cli";
              version = "0.1.0";
              src = srcFiltered;

              nativeBuildInputs =
                [ buildPkgs.pkg-config ]
                ++ pkgs.lib.optionals isHostDarwin [ buildPkgs.libiconv ];

              cargoBuildFlags = [ "--target=${targetTriple}" ];

              env =
                mkReproEnv
                // (pkgs.lib.optionalAttrs (spec.crossSystemConfig != null) {
                  "CC_${rustTargetEnv}" =
                    "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}gcc";
                  "AR_${rustTargetEnv}" =
                    "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}ar";
                  "CARGO_TARGET_${upperTargetEnv}_LINKER" =
                    "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}gcc";
                })
                // {
                  "CARGO_TARGET_${upperTargetEnv}_RUSTFLAGS" = rustFlags;
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
                cp target/${targetTriple}/release/clementine-cli $out/bin/
                chmod 555 $out/bin/clementine-cli
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

                    cargoBuildFlags = [ "--target=${targetTriple}" ];

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
                        --remap-path-prefix=${srcFiltered}=/src \
                        --remap-path-prefix=$NIX_BUILD_TOP=/build";

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
