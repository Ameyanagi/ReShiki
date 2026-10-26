{
  description = "ReShiki: chemical drawing, reinvented";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      # These fixes have not all reached nixos-unstable yet. Keep the upstream
      # sources pinned for the entire package set, including transitive font
      # and HTTP dependencies, rather than patching only ReShiki's direct inputs.
      securityOverlay = _final: prev: {
        brotli = prev.brotli.overrideAttrs (_: {
          version = "1.2.1";
          src = prev.fetchFromGitHub {
            owner = "google";
            repo = "brotli";
            tag = "v1.2.1";
            hash = "sha256-llY72cYffDv0ugsH0p89xIPEbElV4L8u7U/Fe8hT3gI=";
          };
          # The old architecture fix is already included in v1.2.1.
          patches = [ ];
        });

        curlMinimal = prev.curlMinimal.overrideAttrs (_: {
          version = "8.23.0";
          # Preserve nixpkgs' bootstrap fetcher when it builds curl itself.
          src = prev.stdenv.fetchurlBoot {
            url = "https://curl.se/download/curl-8.23.0.tar.xz";
            hash = "sha256-ja3X0lNqMU+YKtBaZRtxP0Jr99Aswsds/7l5byIY5sU=";
          };
        });

        freetype = prev.freetype.overrideAttrs (old: {
          patches = old.patches ++ [
            # CVE-2026-50811: zero excess variation coordinates in both loaders.
            (prev.fetchurl {
              url = "https://github.com/freetype/freetype/commit/8fa928f1617aba65f45b00f0dcb109f077b7741e.patch";
              hash = "sha256-eu0Yr6b99ak0YG2HMUWHTpJScNLcg+S9ZAsC2oGQ0ow=";
            })
            (prev.fetchurl {
              url = "https://github.com/freetype/freetype/commit/5a280ecde6f324de0d226261036e736e0cb49a71.patch";
              hash = "sha256-+uHJbNlLntWq/HQk9LiJTy4jW0hppys827aE3tzqnRo=";
            })
            # CVE-2026-95512: bound overlapping CID subroutine dictionaries.
            (prev.fetchurl {
              url = "https://github.com/freetype/freetype/commit/f3ca71c9900fe860849b3163a6e2c1e765b291d9.patch";
              hash = "sha256-rmGaC+w+gLwnm1DotW1YCULZRGuGNqawE0LDsQiu4TQ=";
            })
          ];
        });
      };
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ securityOverlay ];
            }
          )
        );
    in
    {
      overlays.default = securityOverlay;

      packages = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.reshiki;
        reshiki =
          let
            inherit (pkgs) lib stdenv;
            # Libraries winit/wgpu open at runtime with dlopen.
            runtimeLibs = with pkgs; [
              wayland
              libxkbcommon
              vulkan-loader
              libGL
              libx11
              libxcursor
              libxi
              libxrandr
            ];
          in
          pkgs.rustPlatform.buildRustPackage {
            pname = "reshiki";
            inherit ((lib.importTOML ./Cargo.toml).package) version;
            # Documentation and the website are not build inputs; leaving them
            # out keeps docs-only commits from rebuilding the package.
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.difference ./. (
                lib.fileset.unions [
                  ./docs
                  ./website
                  ./.github
                  ./flake.nix
                  ./flake.lock
                ]
              );
            };
            # Git dependencies are fetched at the rev pinned in Cargo.lock, so
            # bumping COSMolKit needs no hash update here.
            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };
            # The app relaunches itself for InChI, naming and geometry work;
            # the retired reshiki-inchi-helper must not ship.
            cargoBuildFlags = [
              "--bin"
              "reshiki"
            ];

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.python3
            ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [ pkgs.copyDesktopItems ];
            buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
              pkgs.gtk3
              pkgs.libxkbcommon
              pkgs.wayland
            ];

            # Most suites need the Python/RDKit reference; the install check
            # below runs the shipped binary instead.
            doCheck = false;
            doInstallCheck = true;
            installCheckPhase = ''
              runHook preInstallCheck
              export HOME=$TMPDIR
              # InChI goes through the app's own worker mode.
              $out/bin/reshiki --cli analyze --smiles 'c1ccccc1O' \
                | grep -F '"inchi":"InChI=1S/C6H6O/c7-6-4-2-1-3-5-6/h1-5,7H"'
              $out/bin/reshiki --cli convert --smiles 'CCO' -o $TMPDIR/ethanol.png
              test -s $TMPDIR/ethanol.png
              ${lib.optionalString stdenv.hostPlatform.isDarwin ''
                grep -q '<string>dev.reshiki.editor</string>' $out/Applications/ReShiki.app/Contents/Info.plist
                test -x $out/Applications/ReShiki.app/Contents/MacOS/reshiki
              ''}
              runHook postInstallCheck
            '';

            postInstall = ''
              ${lib.optionalString stdenv.hostPlatform.isLinux ''
                for size in 16 24 32 48 64 128 256 512 1024; do
                  install -Dm644 assets/branding/exports/light/icon-$size.png \
                    $out/share/icons/hicolor/''${size}x''${size}/apps/reshiki.png
                done
              ''}

              # NOTICE requires the license texts in binary distributions; reuse
              # the release builder's aggregation over the vendored crates.
              cargo metadata --format-version 1 --locked --offline > "$TMPDIR/metadata.json"
              python3 -c '
              import json, sys
              from pathlib import Path
              sys.path.insert(0, "scripts")
              from license_notices import write_notices
              write_notices(Path("."), Path(sys.argv[1]), json.load(open(sys.argv[2])))
              ' "$out/share/licenses/reshiki" "$TMPDIR/metadata.json"
            '';

            postFixup =
              if stdenv.hostPlatform.isLinux then
                ''
                  patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/reshiki
                ''
              else
                # nix-darwin and home-manager link Applications/*.app into the
                # Applications folder. The binary moves after fixup so it stays
                # stripped and signed; bin/reshiki keeps the command line.
                ''
                  app=$out/Applications/ReShiki.app
                  python3 -c '
                  import sys
                  from pathlib import Path
                  sys.path.insert(0, "scripts")
                  from build_release import bundle_metadata
                  bundle_metadata(Path(sys.argv[1]))
                  ' "$app"
                  install -d $app/Contents/MacOS
                  mv $out/bin/reshiki $app/Contents/MacOS/reshiki
                  ln -s ../Applications/ReShiki.app/Contents/MacOS/reshiki $out/bin/reshiki
                '';

            desktopItems = lib.optionals stdenv.hostPlatform.isLinux [
              (pkgs.makeDesktopItem {
                name = "reshiki";
                desktopName = "ReShiki";
                comment = "Chemical drawing";
                exec = "reshiki --open %f";
                icon = "reshiki";
                categories = [
                  "Science"
                  "Chemistry"
                  "Graphics"
                ];
              })
            ];

            meta = {
              description = "Molecular drawing workspace";
              homepage = "https://reshiki.com";
              license = with lib.licenses; [
                mit
                asl20
              ];
              mainProgram = "reshiki";
              platforms = systems;
            };
          };
      });

      checks = forAllSystems (pkgs: {
        reshiki = self.packages.${pkgs.stdenv.hostPlatform.system}.reshiki;
      });
    };
}
