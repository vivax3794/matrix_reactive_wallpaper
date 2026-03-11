{
  description = "Matrix-style reactive wallpaper for Wayland.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        pkgs = import nixpkgs {
          inherit system;
        };
        runtimeLibs = with pkgs; [
          wayland
          libGL
          mesa
          libxkbcommon
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          doCheck = false;
          nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
          buildInputs = runtimeLibs;

          postInstall = ''
            wrapProgram $out/bin/${cargoToml.package.name} \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
          '';

          meta = {
            description = "Matrix-style reactive wallpaper for Wayland";
            license = pkgs.lib.licenses.mit;
            mainProgram = cargoToml.package.name;
            maintainers = [
              {
                github = "vivax3794";
                email = "vivax3794@protonmail.com";
                name = "Viv";
              }
            ];
          };
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            (fenix.packages.${system}.latest.withComponents [
              "cargo"
              "clippy"
              "rustc"
              "rust-src"
              "rustfmt"
            ])
            pkg-config
          ];
          buildInputs = runtimeLibs;
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        };
      }
    );
}
