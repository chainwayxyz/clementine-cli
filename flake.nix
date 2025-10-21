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
      # Define the build system - this is the machine you're building on
      buildSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      # Define all target platforms we want to build for
      targetSystems = {
        x86_64-linux = { config = "x86_64-unknown-linux-gnu"; rustTarget = "x86_64-unknown-linux-gnu"; };
        aarch64-linux = { config = "aarch64-unknown-linux-gnu"; rustTarget = "aarch64-unknown-linux-gnu"; };
        x86_64-darwin = { config = "x86_64-apple-darwin"; rustTarget = "x86_64-apple-darwin"; };
        aarch64-darwin = { config = "aarch64-apple-darwin"; rustTarget = "aarch64-apple-darwin"; };
        x86_64-windows = { config = "x86_64-w64-mingw32"; rustTarget = "x86_64-pc-windows-gnu"; };
      };
    in
    flake-utils.lib.eachSystem buildSystems (buildSystem:
      let
        overlays = [ (import rust-overlay) ];

        # Function to create a cross-compilation package set
        mkCrossPkgs = targetSystem: targetConfig:
          if buildSystem == targetSystem then
            # Native build - no cross-compilation needed
            import nixpkgs {
              system = buildSystem;
              inherit overlays;
            }
          else
            # Cross-compilation
            import nixpkgs {
              system = buildSystem;
              crossSystem = {
                config = targetConfig.config;
              };
              inherit overlays;
            };

        # Create packages for all target systems
        mkPackagesForTargets = builtins.mapAttrs (targetName: targetConfig:
          let
            pkgs = mkCrossPkgs targetName targetConfig;

            # Pin Rust version to match rust-toolchain.toml for reproducibility
            # This exact version must be kept in sync with rust-toolchain.toml
            rustVersion = "1.89.0";

            # Determine if this is a cross-compilation
            isCross = buildSystem != targetName;
            isWindows = targetName == "x86_64-windows";
            isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
            isLinux = pkgs.stdenv.hostPlatform.isLinux;

            # Get the appropriate Rust toolchain with cross-compilation target
            rust = pkgs.rust-bin.stable.${rustVersion}.default.override {
              targets = [ targetConfig.rustTarget ];
            };

            rustPlatform = pkgs.makeRustPlatform {
              cargo = rust;
              rustc = rust;
            };

            # Build inputs based on target platform
            nativeBuildInputs = with pkgs; [
              pkg-config
            ] ++ pkgs.lib.optionals isWindows [
              pkgs.stdenv.cc
            ];

            buildInputs = with pkgs;
              if isWindows then [
                windows.pthreads
              ] else if isDarwin then [
                darwin.apple_sdk.frameworks.Security
                darwin.apple_sdk.frameworks.SystemConfiguration
                libiconv
              ] else [
                openssl
              ];

          in
          # Helper to create the package definition
          rustPlatform.buildRustPackage rec {
          pname = "clementine-cli";
          version = "0.1.0";

          src = ./.;

          # Disable cargo-auditable for reproducibility (it embeds timestamps)
          auditable = false;

          cargoLock = {
            lockFile = ./Cargo.lock;
            # Hashes for git dependencies (from Cargo.toml [patch.crates-io])
            # These correspond to specific git commits:
            # - bitcoincore-rpc: chainwayxyz/rust-bitcoincore-rpc@5da45109a2de352472a6056ef90a517b66bc106f
            # - secp256k1: rust-bitcoin/rust-secp256k1@4d36fefdddb118425bb9bcf611bb6e4dff306cfc
            #
            # To update these hashes when dependencies change:
            # 1. Update the git rev in Cargo.toml
            # 2. Run: ./contrib/reproducible/update-hashes.sh
            # 3. Copy the new hashes from the error output to here
            # 4. Verify the git commits match what you expect before building
            outputHashes = {
              "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
              "secp256k1-0.31.0" = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
            };
          };

            inherit nativeBuildInputs buildInputs;

            # Set the target for cross-compilation
            CARGO_BUILD_TARGET = if isCross then targetConfig.rustTarget else null;

            # Reproducibility flags for deterministic builds
            # - debuginfo=0: Remove debug info (which can contain non-deterministic paths)
            # - opt-level=3: Maximum optimization
            # - codegen-units=1: Single codegen unit for deterministic code generation
            RUSTFLAGS = "-C debuginfo=0 -C opt-level=3 -C codegen-units=1";

            # Set fixed timestamp for reproducible builds (epoch = 1970-01-01)
            SOURCE_DATE_EPOCH = "1";

            # Disable stripping to ensure deterministic builds
            # Even though we have debuginfo=0, we disable stripping because the strip
            # tool itself can introduce non-determinism in some edge cases
            dontStrip = true;

            # Use single-threaded build for determinism
            # Parallel builds can introduce non-deterministic ordering in the final binary
            enableParallelBuilding = false;

            meta = with pkgs.lib; {
              description = "Clementine CLI tool for ${targetName}";
              homepage = "https://github.com/chainwayxyz/clementine-cli";
              license = licenses.gpl3;
              maintainers = [ ];
              platforms = [ targetName ];
            };
          }
        ) targetSystems;

        # Native package for the current build system
        nativePkgs = import nixpkgs {
          system = buildSystem;
          inherit overlays;
        };

        # Pin Rust version to match rust-toolchain.toml for reproducibility
        rustVersion = "1.89.0";

        nativeRust = nativePkgs.rust-bin.stable.${rustVersion}.default;

      in
      {
        # Expose all cross-compilation packages
        packages = mkPackagesForTargets // {
          # Default package - native build for the current system
          default = mkPackagesForTargets.${buildSystem};
          # Alias for convenience
          clementine-cli = mkPackagesForTargets.${buildSystem};
        };

        # Development shell with native build tools
        devShells.default = nativePkgs.mkShell {
          nativeBuildInputs = with nativePkgs; [
            pkg-config
          ];

          buildInputs = with nativePkgs;
            (if nativePkgs.stdenv.isDarwin then [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.SystemConfiguration
              libiconv
            ] else [
              openssl
            ]) ++ [ nativeRust ];

          shellHook = ''
            echo "Clementine CLI development environment"
            echo "Rust version: ${rustVersion}"
            echo "Build system: ${buildSystem}"
            echo ""
            echo "Available cross-compilation targets:"
            echo "  - x86_64-linux (nix build .#x86_64-linux)"
            echo "  - aarch64-linux (nix build .#aarch64-linux)"
            echo "  - x86_64-darwin (nix build .#x86_64-darwin)"
            echo "  - aarch64-darwin (nix build .#aarch64-darwin)"
            echo "  - x86_64-windows (nix build .#x86_64-windows)"
          '';
        };
      }
    );
}