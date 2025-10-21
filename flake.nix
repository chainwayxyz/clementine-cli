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
      targetSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" "x86_64-windows" ];
    in
    flake-utils.lib.eachSystem buildSystems (buildSystem:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { system = buildSystem; inherit overlays; };

        rustVersion = "1.89.0";
        rustPinned = pkgs.rust-bin.stable.${rustVersion}.default.override {
          targets = [ "x86_64-pc-windows-gnu" ];
        };
        rustPlatformPinned = pkgs.makeRustPlatform { cargo = rustPinned; rustc = rustPinned; };

        isDarwin = pkgs.stdenv.isDarwin;

        mkPackageFor = targetName:
          let
            isWindows = targetName == "x86_64-windows";

            # For Windows, use pkgsCross for proper cross-compilation
            targetPkgs = if isWindows 
              then pkgs.pkgsCross.mingwW64
              else pkgs;

            nativeBuildInputs = [ pkgs.pkg-config ];

            # buildInputs should only contain libraries for the TARGET platform
            # For Windows cross-compilation, we don't add Windows libraries here
            # because they cause build scripts (which run on the host) to try linking against them
            buildInputs =
              if isWindows then
                [ ]
              else if isDarwin then
                [ pkgs.darwin.apple_sdk.frameworks.Security
                  pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
                  pkgs.libiconv ]
              else
                [ pkgs.openssl ];

            cargoBuildFlags = pkgs.lib.optionals isWindows [ "--target" "x86_64-pc-windows-gnu" ];

            # For Windows, we need to customize the install phase since the binary is in a different location
            installPhase = if isWindows then ''
              runHook preInstall
              mkdir -p $out/bin
              cp target/x86_64-pc-windows-gnu/release/clementine-cli.exe $out/bin/
              runHook postInstall
            '' else null;
          in
          rustPlatformPinned.buildRustPackage rec {
            pname = "clementine-cli${pkgs.lib.optionalString isWindows "-x86_64-w64-mingw32"}";
            version = "0.1.0";

            src = ./.;

            inherit nativeBuildInputs buildInputs cargoBuildFlags installPhase;

            # Reproducibility knobs
            auditable = false;
            SOURCE_DATE_EPOCH = "1";
            dontStrip = true;
            enableParallelBuilding = false;

            depsBuildBuild = pkgs.lib.optionals isWindows [ targetPkgs.stdenv.cc ];

            # Critical: Set these environment variables to configure cross-compilation properly
            env = if isWindows then {
              # Tell Cargo where the Windows linker is
              CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER =
                "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";

              # Set rustflags for the Windows target only (not for build scripts)
              # The key is to put library paths in link-args so they only apply to the target
              CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS =
                "-C target-feature=-crt-static -C link-arg=-L${targetPkgs.windows.pthreads}/lib";

              # Ensure build scripts use the host compiler and standard libraries
              CC_x86_64_unknown_linux_gnu = "${pkgs.stdenv.cc}/bin/cc";
              HOST_CC = "${pkgs.stdenv.cc}/bin/cc";

              # Configure for secp256k1-sys and other C dependencies cross-compilation
              TARGET_CC = "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";
              CC_x86_64_pc_windows_gnu = "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";
              AR_x86_64_pc_windows_gnu = "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}ar";
            } else {};

            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes = {
                "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
                "secp256k1-0.31.0"      = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
              };
            };

            doCheck = !isWindows;

            meta = with pkgs.lib; {
              description = "Clementine CLI tool for ${targetName}";
              homepage = "https://github.com/chainwayxyz/clementine-cli";
              license = licenses.gpl3;
              platforms = [ pkgs.stdenv.hostPlatform.system ];
            };
          };

        packagesForAll = pkgs.lib.genAttrs targetSystems mkPackageFor;

      in
      {
        packages = packagesForAll // {
          default = packagesForAll.${buildSystem};
          clementine-cli = packagesForAll.${buildSystem};
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
            echo "Build targets:"
            echo "  nix build .#x86_64-linux"
            echo "  nix build .#aarch64-linux"
            echo "  nix build .#x86_64-darwin"
            echo "  nix build .#aarch64-darwin"
            echo "  nix build .#x86_64-windows"
          '';
        };
      }
    );
}