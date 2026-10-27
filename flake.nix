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
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.reshiki;
        reshiki =
          let
            inherit (pkgs) lib stdenv;
            # Pin the latest stable official compiler and Cargo binaries. Reuse
            # nixpkgs' binary installer instead of rebuilding Rust and LLVM.
            toolchain = pkgs.callPackage (nixpkgs + "/pkgs/development/compilers/rust/bootstrap.nix") {
              version = "1.99.1";
              hashes = {
                x86_64-unknown-linux-gnu = "aded4e6c1d580415e7b594a3080d8e23e9cbe06f159f97e300aa6875211f71bf";
                aarch64-unknown-linux-gnu = "703b190f0d723ae68985797f3bb5f62f01b1723f138464c6f30f32b766eb0abf";
                aarch64-apple-darwin = "b73b6a78c4e7171a8fe1b4bee5342273605c5c9a9a4997554c15bb3ce2753b6a";
              };
            };
            rustPlatform = pkgs.makeRustPlatform toolchain;
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
          rustPlatform.buildRustPackage {
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
                startupWMClass = "reshiki";
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
