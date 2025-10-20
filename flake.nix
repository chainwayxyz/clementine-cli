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
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
      "x86_64-windows"
    ] (system:
      let
        overlays = [ (import rust-overlay) ];

        # For cross-compilation to Windows from Linux
        pkgs = if system == "x86_64-windows" then
          import nixpkgs {
            system = "x86_64-linux";
            crossSystem = {
              config = "x86_64-w64-mingw32";
            };
            inherit overlays;
          }
        else
          import nixpkgs {
            inherit system overlays;
          };

        # Pin Rust version to match rust-toolchain.toml for reproducibility
        # This exact version must be kept in sync with rust-toolchain.toml
        rustVersion = "1.89.0";

        rust = if system == "x86_64-windows" then
          pkgs.pkgsCross.mingwW64.rust-bin.stable.${rustVersion}.default.override {
            targets = [ "x86_64-pc-windows-gnu" ];
          }
        else
          pkgs.rust-bin.stable.${rustVersion}.default;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rust;
          rustc = rust;
        };

        # Build inputs based on target platform
        nativeBuildInputs = with pkgs; [
          pkg-config
        ] ++ pkgs.lib.optionals (system == "x86_64-windows") [
          pkgs.pkgsCross.mingwW64.stdenv.cc
        ];

        buildInputs = with pkgs;
          if system == "x86_64-windows" then [
            pkgs.pkgsCross.mingwW64.windows.pthreads
          ] else if pkgs.stdenv.isDarwin then [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.SystemConfiguration
            libiconv
          ] else [
            openssl
          ];

        # Helper to create the package definition
        buildClementineCli = rustPlatform.buildRustPackage rec {
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

          CARGO_BUILD_TARGET = if system == "x86_64-windows"
            then "x86_64-pc-windows-gnu"
            else null;

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
            description = "Clementine CLI tool";
            homepage = "https://github.com/chainwayxyz/clementine-cli";
            license = licenses.gpl3;
            maintainers = [ ];
          };
        };

      in
      {
        # Default package (accessible via `nix build`)
        packages.default = buildClementineCli;

        # Convenient shorthand (accessible via `nix build .#clementine-cli`)
        packages.clementine-cli = buildClementineCli;

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;
          buildInputs = buildInputs ++ [ rust ];

          shellHook = ''
            echo "Clementine CLI development environment"
            echo "Rust version: ${rustVersion}"
          '';
        };
      }
    );
}