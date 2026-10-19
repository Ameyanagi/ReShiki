{
  description = "ReShiki: chemical drawing, reinvented";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.reshiki;
        reshiki =
          let
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
            inherit ((builtins.fromTOML (builtins.readFile ./Cargo.toml)).package) version;
            src = self;
            cargoLock = {
              lockFile = ./Cargo.lock;
              # Update when the COSMolKit rev in Cargo.lock changes.
              outputHashes."cosmolkit-core-0.3.0" = "sha256-RSzO5Q59vlDkve2XQMJeqJpQnMyFht4WvZ9AhwemQuo=";
            };

            nativeBuildInputs = with pkgs; [
              pkg-config
              patchelf
              python3
              copyDesktopItems
            ];
            buildInputs = with pkgs; [
              gtk3
              libxkbcommon
              wayland
            ];

            # Most suites need the Python/RDKit reference.
            doCheck = false;

            postInstall = ''
              for size in 16 24 32 48 64 128 256 512 1024; do
                install -Dm644 assets/branding/exports/light/icon-$size.png \
                  $out/share/icons/hicolor/''${size}x''${size}/apps/reshiki.png
              done

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

            postFixup = ''
              patchelf --add-rpath ${pkgs.lib.makeLibraryPath runtimeLibs} $out/bin/reshiki
            '';

            desktopItems = [
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
              license = with pkgs.lib.licenses; [
                mit
                asl20
              ];
              mainProgram = "reshiki";
              platforms = systems;
            };
          };
      });
    };
}
