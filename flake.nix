{
  inputs.nixpkgs.url = "https://channels.nixos.org/nixpkgs-unstable/nixexprs.tar.xz";

  outputs = inputs: let
    inherit (inputs.nixpkgs) lib;
    systems = lib.systems.flakeExposed;
    eachSystem = lib.genAttrs systems;
    pkgsFor = system:
      import inputs.nixpkgs {
        inherit system;
      };

    mkPackage = pkgs:
      pkgs.rustPlatform.buildRustPackage {
        pname = "mdbook-treesitter";
        version = "0.1.0";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
      };
  in {
    packages = eachSystem (system: let
      pkgs = pkgsFor system;
    in {
      default = mkPackage pkgs;
      mdbook-treesitter = mkPackage pkgs;
    });

    overlays.default = _final: prev: {
      mdbook-treesitter = mkPackage prev;
    };

    devShells = eachSystem (
      system: let
        pkgs = pkgsFor system;
      in {
        default = pkgs.mkShell {packages = with pkgs; [mdbook tree-sitter];};
      }
    );

    formatter = eachSystem (system: (pkgsFor system).nixfmt);
  };
}
