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
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
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

            postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
              patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/reshiki
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
