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
  in {
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
