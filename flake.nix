{
  description = "Clementine CLI - Reproducible builds";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      buildSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      # Working target systems (PowerPC64 excluded due to Nix cross-compilation limitations)
      targetSystems = [
        "x86_64-linux-gnu"
        "aarch64-linux-gnu"
        "arm-linux-gnueabihf"
        "riscv64-linux-gnu"
        "x86_64-apple-darwin"
        "arm64-apple-darwin"
        "win64"
      ];
    in
    flake-utils.lib.eachSystem buildSystems (buildSystem:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { system = buildSystem; inherit overlays; };

        rustVersion = "1.89.0";
        rustPinned = pkgs.rust-bin.stable.${rustVersion}.default.override {
          targets = [
            "x86_64-pc-windows-gnu"
            "x86_64-unknown-linux-gnu"
            "aarch64-unknown-linux-gnu"
            "arm-unknown-linux-gnueabihf"
            "powerpc64-unknown-linux-gnu"
            "riscv64gc-unknown-linux-gnu"
            "x86_64-apple-darwin"
            "aarch64-apple-darwin"
          ];
        };
        rustPlatformPinned = pkgs.makeRustPlatform { cargo = rustPinned; rustc = rustPinned; };

        isDarwin = pkgs.stdenv.isDarwin;

        # All possible targets - we'll filter by build system below
        allPlatformConfigs = {
          "x86_64-linux-gnu" = {
            rustTarget = "x86_64-unknown-linux-gnu";
            pkgsCross = null;  # Native on x86_64-linux
            isNative = buildSystem == "x86_64-linux";
          };
          "aarch64-linux-gnu" = {
            rustTarget = "aarch64-unknown-linux-gnu";
            pkgsCross = pkgs.pkgsCross.aarch64-multiplatform;
            isNative = buildSystem == "aarch64-linux";
          };
          "arm-linux-gnueabihf" = {
            rustTarget = "arm-unknown-linux-gnueabihf";
            pkgsCross = pkgs.pkgsCross.armv7l-hf-multiplatform;
            isNative = false;
          };
          # NOTE: PowerPC64 is currently not working due to Nix cross-compilation limitations
          # Tested multiple configurations (ppc64, ppc64-elfv2, powernv) - all fail silently
          # Build completes but produces no binary. This is a known Nix/Rust cross-compilation issue.
          # Keeping configuration for future compatibility when upstream fixes are available.
          "powerpc64-linux-gnu" = {
            rustTarget = "powerpc64le-unknown-linux-gnu";
            pkgsCross = pkgs.pkgsCross.powernv;
            isNative = false;
          };
          "riscv64-linux-gnu" = {
            rustTarget = "riscv64gc-unknown-linux-gnu";
            pkgsCross = pkgs.pkgsCross.riscv64;
            isNative = false;
          };
          "x86_64-apple-darwin" = {
            rustTarget = "x86_64-apple-darwin";
            # On macOS, we can cross-compile between architectures using the same SDK
            # Don't use pkgsCross - it's not needed and causes Nix LibsystemCross errors
            # Instead, we use custom compiler wrappers with -arch flags (see darwinLinkerWrapper below)
            pkgsCross = null;
            isNative = buildSystem == "x86_64-darwin";
            # Special flag for Darwin cross-arch builds (ARM64→x86_64 or x86_64→ARM64)
            isDarwinCross = (buildSystem == "aarch64-darwin" || buildSystem == "x86_64-darwin") && buildSystem != "x86_64-darwin";
          };
          "arm64-apple-darwin" = {
            rustTarget = "aarch64-apple-darwin";
            # On macOS, we can cross-compile between architectures using the same SDK
            pkgsCross = null;
            isNative = buildSystem == "aarch64-darwin";
            # Special flag for Darwin cross-arch builds
            isDarwinCross = (buildSystem == "aarch64-darwin" || buildSystem == "x86_64-darwin") && buildSystem != "aarch64-darwin";
          };
          "win64" = {
            rustTarget = "x86_64-pc-windows-gnu";
            pkgsCross = pkgs.pkgsCross.mingwW64;
            isNative = false;
          };
        };

        # Filter platforms based on what can be built on current build system
        # Linux can build: x86_64, ARM64, ARMv7, RISC-V + Windows + macOS (experimental)
        # macOS can build: both macOS architectures using the universal Apple SDK
        # Note: PowerPC64 is excluded due to known Nix cross-compilation limitations
        availableTargets =
          if pkgs.stdenv.isLinux then
            [ "x86_64-linux-gnu" "aarch64-linux-gnu" "arm-linux-gnueabihf"
              "riscv64-linux-gnu" "win64" "x86_64-apple-darwin" "arm64-apple-darwin" ]
          else if pkgs.stdenv.isDarwin then
            # macOS can build both architectures using the same SDK
            [ "x86_64-apple-darwin" "arm64-apple-darwin" ]
          else
            [ ];

        platformConfig = builtins.listToAttrs (map (name: {
          inherit name;
          value = allPlatformConfigs.${name};
        }) availableTargets);

        mkPackageFor = targetName:
          let
            config = platformConfig.${targetName};
            rustTarget = config.rustTarget;
            isNative = config.isNative;
            # For Darwin, cross-arch builds (ARM64↔x86_64) should be treated as cross-compilation
            isDarwinCross = config.isDarwinCross or false;
            isCross = !isNative;
            isWindows = targetName == "win64";
            isDarwinTarget = builtins.elem targetName [ "x86_64-apple-darwin" "arm64-apple-darwin" ];
            isLinuxTarget = builtins.elem targetName [ "x86_64-linux-gnu" "aarch64-linux-gnu" "arm-linux-gnueabihf" "powerpc64-linux-gnu" "riscv64-linux-gnu" ];

            # Use pkgsCross for cross-compilation, otherwise use native pkgs
            targetPkgs = if config.pkgsCross != null then config.pkgsCross else pkgs;

            # Create a linker wrapper for Darwin cross-arch builds
            # This enables cross-compilation between Intel and Apple Silicon on the same Mac
            # We use system clang directly because Nix's cc-wrapper injects build-host specific
            # flags (like -mcpu=armv8.3-a) that conflict with the target architecture
            darwinLinkerWrapper = if isDarwinCross then
              let
                arch = if targetName == "x86_64-apple-darwin" then "x86_64" else "arm64";
              in
              pkgs.writeShellScript "darwin-cross-linker" ''
                #!/bin/bash
                # Use system clang directly with -arch to specify target architecture
                # This works because macOS SDK is universal and supports both architectures
                exec /usr/bin/clang -arch ${arch} "$@"
              ''
            else null;

            # Rust target with underscores for environment variables
            rustTargetEnv = builtins.replaceStrings ["-"] ["_"] rustTarget;

            nativeBuildInputs = [ pkgs.pkg-config ];

            # buildInputs should only contain libraries for the TARGET platform
            buildInputs =
              if isWindows then
                [ ]
              else if isDarwinTarget then
                # For Darwin targets (both native and cross-arch)
                # Use native pkgs frameworks - they work for both architectures
                (if isDarwinCross || isNative then
                  # Native or Darwin cross-arch build - use native frameworks
                  [ pkgs.darwin.apple_sdk.frameworks.Security
                    pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
                    pkgs.libiconv ]
                else
                  # Cross-compiling to macOS from Linux is experimental
                  [ ])
              else if isLinuxTarget then
                [ pkgs.openssl ]
              else
                [ ];

            cargoBuildFlags = pkgs.lib.optionals isCross [ "--target" rustTarget ];

            # Custom install phase for cross-compilation
            installPhase = if isCross then ''
              runHook preInstall
              mkdir -p $out/bin
              cp target/${rustTarget}/release/clementine-cli${pkgs.lib.optionalString isWindows ".exe"} $out/bin/
              runHook postInstall
            '' else null;

            # Cross-compilation environment setup
            crossEnv = if isCross then {
              # Tell Cargo where the cross-compilation linker is
              "CARGO_TARGET_${pkgs.lib.toUpper rustTargetEnv}_LINKER" =
                if isDarwinCross then
                  # For Darwin cross-arch, use our wrapper that forces the right architecture
                  "${darwinLinkerWrapper}"
                else if isDarwinTarget then
                  # For other Darwin cross-compilation (e.g., from Linux)
                  "cc"
                else
                  "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";

              # Rust flags for the target
              "CARGO_TARGET_${pkgs.lib.toUpper rustTargetEnv}_RUSTFLAGS" =
                if isWindows then
                  "-C target-feature=-crt-static -C link-arg=-L${targetPkgs.windows.pthreads}/lib"
                else
                  # No additional flags needed - the linker wrapper handles Darwin cross-arch
                  "";

              # Ensure build scripts use the host compiler
              HOST_CC = "${pkgs.stdenv.cc}/bin/cc";

              # Configure for C dependencies cross-compilation
            } // (if isDarwinTarget then
              # For Darwin cross-arch, we need to set flags for the C compiler too
              (if isDarwinCross then
                let
                  arch = if targetName == "x86_64-apple-darwin" then "x86_64" else "arm64";
                  # Create a CC wrapper for C dependencies (like ring's curve25519.c)
                  # This ensures C code compiled during build.rs is also for the correct architecture
                  ccWrapper = pkgs.writeShellScript "cc-wrapper-${arch}" ''
                    #!/bin/bash
                    # Use system clang to compile C code for the target architecture
                    exec /usr/bin/clang -arch ${arch} "$@"
                  '';
                in {
                  TARGET_CC = "${ccWrapper}";
                  "CC_${rustTargetEnv}" = "${ccWrapper}";
                  "AR_${rustTargetEnv}" = "${pkgs.stdenv.cc}/bin/ar";
                  "CFLAGS_${rustTargetEnv}" = "-arch ${arch}";
                }
              else {})
            else {
              TARGET_CC = "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";
              "CC_${rustTargetEnv}" = "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";
              "AR_${rustTargetEnv}" = "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}ar";
            }) else {};
          in
          rustPlatformPinned.buildRustPackage rec {
            pname = "clementine-cli-${targetName}";
            version = "0.1.0";

            src = ./.;

            inherit nativeBuildInputs buildInputs cargoBuildFlags installPhase;

            # Reproducibility knobs
            auditable = false;
            SOURCE_DATE_EPOCH = "1";
            dontStrip = true;
            enableParallelBuilding = false;

            depsBuildBuild = pkgs.lib.optionals isCross [ targetPkgs.stdenv.cc ];

            env = crossEnv;

            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
                "secp256k1-0.31.0"      = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
              };
            };

            doCheck = !isCross;

            meta = with pkgs.lib; {
              description = "Clementine CLI tool for ${targetName}";
              homepage = "https://github.com/chainwayxyz/clementine-cli";
              license = licenses.gpl3;
              platforms = [ pkgs.stdenv.hostPlatform.system ];
            };
          };

        packagesForAll = pkgs.lib.genAttrs availableTargets mkPackageFor;

        # Map build system to target name
        defaultTarget = {
          "x86_64-linux" = "x86_64-linux-gnu";
          "aarch64-linux" = "aarch64-linux-gnu";
          "x86_64-darwin" = "x86_64-apple-darwin";
          "aarch64-darwin" = "arm64-apple-darwin";
        }.${buildSystem};

      in
      {
        packages = packagesForAll // {
          default = packagesForAll.${defaultTarget};
          clementine-cli = packagesForAll.${defaultTarget};
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs =
            (if isDarwin then [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
              pkgs.libiconv
            ] else [
              pkgs.openssl
            ]) ++ [ rustPinned ];

          shellHook = ''
            echo "Clementine CLI development environment"
            echo "Rust version: ${rustVersion}"
            echo "Build system: ${buildSystem}"
            echo
            echo "Available build targets on this system:"
            ${pkgs.lib.concatMapStringsSep "\n" (target: ''echo "  nix build .#${target}"'') availableTargets}
            echo
            ${if isDarwin then ''
            echo "Note: macOS can build both Intel and Apple Silicon architectures."
            echo "      For Linux/Windows builds, use a Linux system."
            '' else ''
            echo "Note: Linux can build all targets including macOS."
            ''}
          '';
        };
      }
    );
}