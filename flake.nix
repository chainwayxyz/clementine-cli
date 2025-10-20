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

        # Pin Rust version to match rust-toolchain.toml
        rustVersion = "1.89.0";

        rust = if system == "x86_64-windows" then
          pkgs.pkgsCross.mingwW64.rust-bin.stable.${rustVersion}.default.override {
            targets = [ "x86_64-pc-windows-gnu" ];
          }
        else
          pkgs.rust-bin.stable.${rustVersion}.default;

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

      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage rec {
          pname = "clementine-cli";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            # Note: These hashes need to be updated on first build
            # Run: contrib/reproducible/update-hashes.sh
            # Or manually update after seeing the error with correct hashes
            outputHashes = {
              # Placeholder hashes - will be updated during first build
              "bitcoincore-rpc-0.18.0" = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
              "secp256k1-0.31.0" = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
            };
          };

          inherit nativeBuildInputs buildInputs;

          # Set target for Windows cross-compilation
          CARGO_BUILD_TARGET = if system == "x86_64-windows"
            then "x86_64-pc-windows-gnu"
            else null;

          # Ensure reproducible builds
          RUSTFLAGS = "-C debuginfo=0 -C opt-level=3";

          # Set SOURCE_DATE_EPOCH for reproducibility
          SOURCE_DATE_EPOCH = "1";

          meta = with pkgs.lib; {
            description = "Clementine CLI tool";
            homepage = "https://github.com/chainwayxyz/clementine-cli";
            license = licenses.gpl3;
            maintainers = [ ];
          };
        };

        # Development shell
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
