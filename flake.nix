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
    in
    flake-utils.lib.eachSystem buildSystems (buildSystem:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { system = buildSystem; inherit overlays; };

        rustVersion = "1.89.0";
        rust = pkgs.rust-bin.stable.${rustVersion}.default;

        allTargets = {
          linux-x86_64 = {
            cargoTarget = "x86_64-unknown-linux-gnu";
            buildOn = [ "x86_64-linux" ];
            pkgsCross = null;
          };
          linux-aarch64 = {
            cargoTarget = "aarch64-unknown-linux-gnu";
            buildOn = [ "aarch64-linux" ]; 
            pkgsCross = null;
          };
          darwin-x86_64 = {
            cargoTarget = "x86_64-apple-darwin";
            buildOn = [ "x86_64-darwin" ];
            pkgsCross = null;
          };
          darwin-aarch64 = {
            cargoTarget = "aarch64-apple-darwin";
            buildOn = [ "aarch64-darwin" ];
            pkgsCross = null;
          };
          windows-x86_64 = {
            cargoTarget = "x86_64-pc-windows-gnu";
            buildOn = [ "x86_64-linux" ];
            pkgsCross = pkgs.pkgsCross.mingwW64;
          };
        };

        allowed = builtins.filter (name:
          builtins.elem buildSystem allTargets.${name}.buildOn) (builtins.attrNames allTargets);

        rustWithTargets = rust.override {
          targets = builtins.map (n: allTargets.${n}.cargoTarget) allowed;
        };

        rustPlatform = pkgs.makeRustPlatform { cargo = rustWithTargets; rustc = rustWithTargets; };

        mkPackageFor = targetName:
          let
            cfg = allTargets.${targetName};
            rustTarget = cfg.cargoTarget;
            isWindows = rustTarget == "x86_64-pc-windows-gnu";
            isDarwin = builtins.match ".*-apple-darwin" rustTarget != null;
            isLinux = builtins.match ".*-unknown-linux-gnu" rustTarget != null;

            srcFiltered = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type:
                let base = baseNameOf path; in
                ! (base == ".git" || base == ".github" || base == "docs" || base == "README.md");
            };

            targetPkgs = if cfg.pkgsCross != null then cfg.pkgsCross else pkgs;

            buildInputs =
              pkgs.lib.optionals isDarwin [
                pkgs.darwin.apple_sdk.frameworks.Security
                pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
                pkgs.libiconv
              ] ++
              pkgs.lib.optionals isLinux [ pkgs.openssl ];

            nativeBuildInputs =
               (if isWindows then [ targetPkgs.buildPackages.binutils ] else []) ++
               [ pkgs.pkg-config ];

            rustTargetEnv = builtins.replaceStrings ["-"] ["_"] rustTarget;

             crossEnv = if cfg.pkgsCross != null then
               let
                 TUP = rustTarget;
                 TUP_U = pkgs.lib.toUpper rustTargetEnv;
                 prefix = targetPkgs.stdenv.cc.bintools.targetPrefix;
                 binutils = "${targetPkgs.buildPackages.binutils}/bin";
                 gccBin   = "${targetPkgs.stdenv.cc}/bin";
                 baseRustFlags =
                   "-C codegen-units=1 -C embed-bitcode=no -C debuginfo=0 -C lto=off" +
                   " --remap-path-prefix=${srcFiltered}=/src" +
                   " --remap-path-prefix=$NIX_BUILD_TOP=/build" +
                   (if isWindows then " -C target-feature=-crt-static -C link-arg=-L${targetPkgs.windows.pthreads}/lib" else "") +
                   (if isDarwin  then " -C link-arg=-Wl,-oso_prefix,$(realpath $NIX_BUILD_TOP)/" else "");
               in {
                 "CARGO_TARGET_${TUP_U}_LINKER" = "${gccBin}/${prefix}gcc";
                 "CARGO_TARGET_${TUP_U}_AR"     = "${binutils}/${prefix}ar";
                 "CARGO_TARGET_${TUP_U}_RANLIB" = "${binutils}/${prefix}ranlib";
                 "AR_${TUP_U}"      = "${binutils}/${prefix}ar";
                 "RANLIB_${TUP_U}"  = "${binutils}/${prefix}ranlib";
                 "DLLTOOL_${TUP_U}" = "${binutils}/${prefix}dlltool";
                 "CARGO_TARGET_${TUP_U}_RUSTFLAGS" = baseRustFlags;
                 HOST_CC = "${pkgs.stdenv.cc}/bin/cc";
               }
             else {};

            cargoBuildFlags = pkgs.lib.optionals (cfg.pkgsCross != null) [ "--target" rustTarget ];

          in
          (rustPlatform.buildRustPackage rec {
            pname = "clementine-cli-${targetName}";
            version = "0.1.0";

            src = srcFiltered;

            inherit nativeBuildInputs buildInputs cargoBuildFlags;

            env = crossEnv // {
              SOURCE_DATE_EPOCH = "1";
              CARGO_INCREMENTAL = "0";
              ZERO_AR_DATE = "1";
            };

            preBuild = ''
              BASE_RUSTFLAGS="-C codegen-units=1 -C debuginfo=0 -C lto=off -C embed-bitcode=no"
              BASE_RUSTFLAGS="$BASE_RUSTFLAGS --remap-path-prefix=$NIX_BUILD_TOP=/build"
              BASE_RUSTFLAGS="$BASE_RUSTFLAGS --remap-path-prefix=${src}=/src"
              
              ${pkgs.lib.optionalString isDarwin ''
              BASE_RUSTFLAGS="$BASE_RUSTFLAGS -C link-arg=-Wl,-oso_prefix,$(realpath $NIX_BUILD_TOP)/"
              ''}
              
              export RUSTFLAGS="$BASE_RUSTFLAGS"
              export NIX_CFLAGS_COMPILE="$NIX_CFLAGS_COMPILE -fdebug-prefix-map=$NIX_BUILD_TOP=/build -fdebug-prefix-map=${src}=/src"
            '';

            depsBuildBuild = pkgs.lib.optionals (cfg.pkgsCross != null && isWindows) [
              targetPkgs.stdenv.cc
            ];

            cargoLock = { 
              lockFile = ./Cargo.lock;
              outputHashes = {
                "bitcoincore-rpc-0.18.0" = "sha256-QYtvsul7MUFm/HUDAqiwxM4HoFyOcn31ERR8eu62LB4=";
                "secp256k1-0.31.0"       = "sha256-jTdc0423m9lS4NunLCMwLM6AdkerSc/ovTSyO91KXa0=";
              };
            };

            installPhase = ''
              runHook preInstall
              mkdir -p $out/bin
              binName="clementine-cli"
              if [ -f target/${rustTarget}/release/$binName${pkgs.lib.optionalString isWindows ".exe"} ]; then
                cp target/${rustTarget}/release/$binName${pkgs.lib.optionalString isWindows ".exe"} $out/bin/
              else
                cp target/release/$binName${pkgs.lib.optionalString isWindows ".exe"} $out/bin/
              fi
              chmod 555 $out/bin/$binName${pkgs.lib.optionalString isWindows ".exe"}
              runHook postInstall
            '';

            doCheck = false;
            auditable = false;
            dontStrip = true;
          });

        pkgsForThisBuilder = pkgs.lib.genAttrs allowed mkPackageFor;

        defaultTarget = {
          "x86_64-linux" = "linux-x86_64";
          "aarch64-linux" = "linux-aarch64";
          "x86_64-darwin" = "darwin-x86_64";
          "aarch64-darwin" = "darwin-aarch64";
        }.${buildSystem};

      in {
        packages = pkgsForThisBuilder // {
          default = pkgsForThisBuilder.${defaultTarget};
        };

        devShells.default = pkgs.mkShell {
          buildInputs =
            (if pkgs.stdenv.isDarwin then [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
              pkgs.libiconv
            ] else [ pkgs.openssl ])
            ++ [ rustWithTargets pkgs.pkg-config ];

          shellHook = ''
            echo "\nDev env ready for ${buildSystem}"
            echo "Rust ${rustVersion} with targets: ${builtins.concatStringsSep ", " (builtins.map (n: allTargets.${n}.cargoTarget) allowed)}"
            echo "Examples:"
            echo "  nix build .#${defaultTarget}          "
            for t in ${builtins.concatStringsSep " " allowed}; do
              echo "  nix build .#''${t}"
            done
          '';
        };
      }
    );
}