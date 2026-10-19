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
              install -Dm644 assets/branding/app-icon-light.png \
                $out/share/icons/hicolor/1254x1254/apps/reshiki.png
            '';

            postFixup = ''
              patchelf --add-rpath ${pkgs.lib.makeLibraryPath runtimeLibs} $out/bin/reshiki
            '';

            desktopItems = [
              (pkgs.makeDesktopItem {
                name = "reshiki";
                desktopName = "ReShiki";
                comment = "Chemical drawing";
                exec = "reshiki %F";
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
