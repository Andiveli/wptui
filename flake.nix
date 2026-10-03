{
  description = "A keyboard-driven WhatsApp client for the terminal";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      forAllSystems =
        callback:
        nixpkgs.lib.genAttrs [
          "x86_64-linux"
          "aarch64-linux"
        ] (system: callback nixpkgs.legacyPackages.${system});

      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          inherit (pkgs) lib;
        in
        rec {
          whatsrust-libgo = pkgs.buildGoModule {
            pname = "whatsrust-libgo";
            inherit version;

            src = ./whatsrust/lib;

            buildPhase = ''
              runHook preBuild

              go build \
                -buildmode=c-archive \
                -o libgo.a

              runHook postBuild
            '';

            installPhase = ''
              mkdir -p $out/lib
              cp libgo.a $out/lib/
            '';

            vendorHash = "sha256-dGQJJTuiPqp+s9leWl02lbm58loA9yb7gQZEYtDH7tk=";
          };

          default = pkgs.rustPlatform.buildRustPackage (finalAttrs: {
            pname = "wp-tui";
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            strictDeps = true;
            __structuredAttrs = true;

            preBuild = ''
              export WHATSRUST_LIBGO=${whatsrust-libgo}/lib/libgo.a
            '';

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = with pkgs; [
              chafa
              glib
              wayland
              mpv
            ];

            meta = {
              description = "A keyboard-driven WhatsApp client for the terminal";
              homepage = "https://github.com/Andiveli/wptui";
              license = lib.licenses.mit;
              mainProgram = "wp-tui";
              platforms = lib.platforms.linux;
            };
          });

          whatsrust = default;
        }
      );
    };
}
