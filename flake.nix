{
  description = "Clementine CLI – Reproducible cross build for x86_64-pc-windows on x86_64-linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";          # Nixpkgs with relatively up-to-date packages
    flake-utils.url = "github:numtide/flake-utils";            # Utilities for flake output structure
    rust-overlay = {
      url = "github:oxalica/rust-overlay";                     # Rust toolchains overlay (Oxalica)
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (buildSystem:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { system = buildSystem; inherit overlays; };

        rustVersion = "1.89.0";
        targetTriple = "x86_64-pc-windows-gnu";

        # Set up a cross-compilation Nix environment for the Windows target
        crossPkgs = import nixpkgs {
          system = buildSystem;
          crossSystem = { config = "x86_64-w64-mingw32"; };
          inherit overlays;
        };

        buildPkgs = crossPkgs.buildPackages;  # native (Linux) packages

        rust = buildPkgs.rust-bin.stable.${rustVersion}.default.override {
          targets = [ targetTriple ];
        };

        rustPlatform = crossPkgs.makeRustPlatform {
          cargo = rust;  # native tool
          rustc  = rust; # native tool
        };

        # Cleaned source for stable hashing + less noise
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

        rustTargetEnv = builtins.replaceStrings ["-"] ["_"] targetTriple;
      in {
        packages = {
          # Define the Windows cross-compiled package
          windows-x86_64 = rustPlatform.buildRustPackage rec {
            pname = "clementine-cli";
            version = "0.1.0";
            src = srcFiltered;

            # Build inputs and dependencies
            nativeBuildInputs = [ buildPkgs.pkg-config ];        # needed for build scripts that use pkg-config

            buildInputs = [ crossPkgs.windows.mingw_w64_pthreads ];

            cargoBuildFlags = [ "--target=${targetTriple}" ];

            # Ensure Cargo uses the correct cross linker and set reproducibility flags
            env = {
              "CC_${rustTargetEnv}" =
                  "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}gcc";
              "AR_${rustTargetEnv}" =
                  "${crossPkgs.stdenv.cc}/bin/${crossPkgs.stdenv.cc.targetPrefix}ar";

              "CARGO_TARGET_${pkgs.lib.toUpper rustTargetEnv}_LINKER" = "${crossPkgs.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
              # RUSTFLAGS for target: disable timestamp, set deterministic options
              "CARGO_TARGET_${pkgs.lib.toUpper rustTargetEnv}_RUSTFLAGS" = 
               "-L native=${crossPkgs.windows.mingw_w64_pthreads}/lib \
                -C link-arg=-lwinpthread \
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
                --remap-path-prefix=${srcFiltered}=/src \
                --remap-path-prefix=$NIX_BUILD_TOP=/build";
        

              # Environment for reproducible builds
              SOURCE_DATE_EPOCH = "1";
              CARGO_INCREMENTAL = "0";
              ZERO_AR_DATE = "1";
            };

            # Verify Cargo.lock dependencies with fixed hashes for reproducibility
            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                # (example of pinning specific crate outputs for reproducibility)
                "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
                "secp256k1-0.31.0" = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
              };
            };

            # Installation: copy the built .exe to $out/bin
            installPhase = ''
              mkdir -p $out/bin
              cp target/${targetTriple}/release/clementine-cli.exe $out/bin/
              chmod 555 $out/bin/clementine-cli.exe
            '';

            doCheck = false;    # skip tests for cross-compilation (optional)
            auditable = false;  # do not include extra cargo audit metadata
            dontStrip = true;
          };

          # Make this the default package output for convenience
          default = self.packages.${buildSystem}.windows-x86_64;
        };
      }
    );
}
