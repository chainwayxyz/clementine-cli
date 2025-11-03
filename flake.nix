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

      # Working target systems
      # 7 platforms verified reproducible - see docs/reproducible-builds.md for details
      targetSystems = [
        "x86_64-linux-gnu"
        "aarch64-linux-gnu"
        "arm-linux-gnueabihf"
        "riscv64-linux-gnu"
        "x86_64-apple-darwin"
        "arm64-apple-darwin"
        "win64"
      ];

      # Note: PowerPC64 is excluded due to known Nix cross-compilation limitations
      # See docs/reproducible-builds.md "PowerPC64 Known Issue" section for details
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
            pkgsCross = null; # Native on x86_64-linux
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
        # Linux can build: All Linux architectures (x86_64, ARM64, ARMv7, RISC-V) + Windows
        # macOS can build: Both macOS architectures (Intel & Apple Silicon) using the universal Apple SDK
        # See docs/reproducible-builds.md for detailed cross-compilation matrix
        availableTargets =
          if pkgs.stdenv.isLinux then
            [ "x86_64-linux-gnu" "aarch64-linux-gnu" "arm-linux-gnueabihf"
              "riscv64-linux-gnu" "win64" ]
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
            isLinuxTarget = builtins.elem targetName [ "x86_64-linux-gnu" "aarch64-linux-gnu" "arm-linux-gnueabihf" "riscv64-linux-gnu" ];

            # Use pkgsCross for cross-compilation, otherwise use native pkgs
            targetPkgs = if config.pkgsCross != null then config.pkgsCross else pkgs;

            # --- HERMETIC DARWIN BUILD SETUP ---
            # Use a hermetic SDK from Nixpkgs instead of the host's
            appleSdk = pkgs.darwin.apple_sdk_11_0;
            # Create a hermetic stdenv by applying the SDK
            hermeticStdenv = pkgs.stdenvAdapters.useSdk appleSdk targetPkgs.stdenv;
            # Select the stdenv for this build: hermetic for Darwin, standard for others
            stdenvForBuild = if isDarwinTarget then hermeticStdenv else targetPkgs.stdenv;
            # Define paths to hermetic tools
            hermeticClang = "${appleSdk.toolchain}/bin/clang";
            hermeticAr = "${appleSdk.toolchain}/bin/ar";
            # --- END HERMETIC SETUP ---

            # Create a linker wrapper for Darwin cross-arch builds
            # This enables cross-compilation between Intel and Apple Silicon on the same Mac
            darwinLinkerWrapper = if isDarwinCross then
              let
                arch = if targetName == "x86_64-apple-darwin" then "x86_64" else "arm64";
              in
              pkgs.writeShellScript "darwin-cross-linker" ''
                #!/bin/bash
                # Use HERMETIC clang from nixpkgs SDK with explicit arch
                exec ${hermeticClang} -arch ${arch} "$@"
              ''
            else null;

            # Rust target with underscores for environment variables
            rustTargetEnv = builtins.replaceStrings ["-"] ["_"] rustTarget;

            # Add hermetic toolchain for Darwin builds
            nativeBuildInputs = [ pkgs.pkg-config ] ++ pkgs.lib.optionals isDarwinTarget [
              appleSdk.toolchain
            ];

            # buildInputs should only contain libraries for the TARGET platform
            buildInputs =
              if isWindows then
                [ ]
              else if isDarwinTarget then
                # For Darwin targets (both native and cross-arch)
                # Use hermetic SDK frameworks
                [ appleSdk.frameworks.Security
                  appleSdk.frameworks.SystemConfiguration
                  pkgs.libiconv ]
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
                    # Use HERMETIC clang from nixpkgs SDK
                    exec ${hermeticClang} -arch ${arch} "$@"
                  '';
                in {
                  TARGET_CC = "${ccWrapper}";
                  "CC_${rustTargetEnv}" = "${ccWrapper}";
                  "AR_${rustTargetEnv}" = "${hermeticAr}";
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

            # Pass the correct stdenv (hermetic for Darwin) to the builder
            stdenv = stdenvForBuild;

            # Only include files that affect the build
            # Exclude docs, CI, and scripts so documentation changes don't affect build hash
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                let
                  baseName = baseNameOf path;
                  relativePath = pkgs.lib.removePrefix (toString ./. + "/") (toString path);
                in
                # Exclude non-build-affecting directories
                !(pkgs.lib.hasPrefix "docs/" relativePath) &&
                !(pkgs.lib.hasPrefix ".github/" relativePath) &&
                !(pkgs.lib.hasPrefix "reproducible/" relativePath) &&
                !(pkgs.lib.hasPrefix "result/" relativePath) &&
                !(pkgs.lib.hasPrefix "target/" relativePath) &&

                # Exclude specific non-build-affecting files
                !(baseName == "README.md") &&
                !(baseName == "codespell_ignore.txt") &&
                !(baseName == "COPYING") &&
                # Exclude git files
                !(baseName == ".git") &&
                !(baseName == ".gitignore");
            };

            inherit nativeBuildInputs buildInputs cargoBuildFlags installPhase;

            # Reproducibility knobs
            # These settings ensure bit-for-bit identical builds across different machines
            # See docs/reproducible-builds.md "Verifying Reproducibility" for testing
            auditable = false;
            SOURCE_DATE_EPOCH = "1";
            dontStrip = true;
            enableParallelBuilding = false;

            depsBuildBuild = pkgs.lib.optionals isCross [ targetPkgs.stdenv.cc ];

            env = crossEnv;

            cargoLock = {
              lockFile = ./Cargo.lock;
              # Git dependency hashes for reproducibility
              # Update these when git dependencies change using: ./reproducible/update-hashes.sh
              # See docs/reproducible-builds.md "Dependency Hash Updates" for details
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
              # devShell can keep using impure host SDK for convenience
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
            echo "Note: Linux can build all Linux/Windows targets."
            ''}
          '';
        };
      }
    );
}