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
            cargoTarget = "x86_64-unknown-linux-musl";
            buildOn = [ "x86_64-linux" ];
            pkgsCross = null;
          };
          aarch64-linux-gnu = {
            cargoTarget = "aarch64-unknown-linux-musl";
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
            isLinux = builtins.match ".*-unknown-linux-musl" rustTarget != null;
            isStaticDarwinAarch64 = rustTarget == "aarch64-apple-darwin" && buildSystem == "aarch64-darwin";

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
              ] ++
              pkgs.lib.optionals isLinux [ pkgs.openssl ];

            cargoBuildFlags = [ "--target" rustTarget ];

            nativeBuildInputs = [ pkgs.pkg-config ];

            rustTargetEnv = builtins.replaceStrings ["-"] ["_"] (pkgs.lib.toUpper rustTarget);

            commonRustFlags = [
              "-C" "codegen-units=1"
              "-C" "debuginfo=0"
              "-C" "lto=off"
              "-C" "embed-bitcode=no"
              "--remap-path-prefix=${srcFiltered}=/src"
              "--remap-path-prefix=$NIX_BUILD_TOP=/build"
            ];

            darwinRustFlags = pkgs.lib.optionals isDarwin [
              "-C" "target-feature=+crt-static"
              "-C" "link-arg=-Wl,-oso_prefix,/build/"
            ];

            windowsRustFlags = pkgs.lib.optionals isWindows [
              "-C" "link-arg=-Wl,--no-insert-timestamp"
              "-C" "link-arg=-Wl,--sort-section=name"
              "-C" "link-arg=-Wl,--sort-common"
              "-C" "link-arg=-Wl,--build-id=none"
              "-C" "link-arg=-Wl,-s"
              "-C" "metadata=clementine-repro"
              "-C" "link-arg=-L${targetPkgs.windows.pthreads}/lib"
            ];

            finalRustFlags = commonRustFlags ++ darwinRustFlags ++ windowsRustFlags;

            buildEnv = {
              SOURCE_DATE_EPOCH = "1";
              CARGO_INCREMENTAL = "0";
              ZERO_AR_DATE = "1";
              STATIC_LINK_VERSION = "5";

              "CARGO_TARGET_${rustTargetEnv}_RUSTFLAGS" =
                builtins.concatStringsSep " " finalRustFlags;
            } // (pkgs.lib.optionalAttrs (cfg.pkgsCross != null) {
              "CARGO_TARGET_${rustTargetEnv}_LINKER" =
                "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}gcc";
              "CC_${rustTargetEnv}" =
                "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}cc";
              "AR_${rustTargetEnv}" =
                "${targetPkgs.stdenv.cc}/bin/${targetPkgs.stdenv.cc.targetPrefix}ar";
            });

          in
          (rustPlatform.buildRustPackage rec {
            pname = "clementine-cli-${targetName}";
            version = "0.1.0";

            src = srcFiltered;

            inherit nativeBuildInputs buildInputs cargoBuildFlags;

            env = buildEnv;

            preBuild = ''
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

            postInstall =
              pkgs.lib.optionalString isWindows ''
                ${targetPkgs.stdenv.cc.bintools.bintools}/bin/${targetPkgs.stdenv.cc.targetPrefix}strip \
                  --strip-all \
                  --remove-section=.symtab \
                  --remove-section=.strtab \
                  $out/bin/clementine-cli.exe 2>/dev/null || true

                ${targetPkgs.stdenv.cc.bintools.bintools}/bin/${targetPkgs.stdenv.cc.targetPrefix}objcopy \
                  --remove-section=.debug_info \
                  --remove-section=.debug_abbrev \
                  --remove-section=.debug_line \
                  --remove-section=.debug_str \
                  $out/bin/clementine-cli.exe 2>/dev/null || true
              '' +
              pkgs.lib.optionalString isDarwin ''
                chmod +w $out/bin/clementine-cli
                LIBICONV_PATH=$(otool -L $out/bin/clementine-cli | grep libiconv.2.dylib | awk '{print $1}')
                if [ -n "$LIBICONV_PATH" ]; then
                  ${pkgs.darwin.cctools}/bin/install_name_tool \
                    -change "$LIBICONV_PATH" /usr/lib/libiconv.2.dylib \
                    $out/bin/clementine-cli
                fi
                chmod 555 $out/bin/clementine-cli
              '';

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
          "aarch64-linux" = "aarch64-linux-gnu";
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
