{
  description = "Show details about outdated packages in your NixOS system";

  inputs = {
    naersk = {
      url = "github:nix-community/naersk/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    let
      inherit (nixpkgs) lib;
      oldeLambda = pkgs:
        let
          naersk-lib = pkgs.callPackage naersk { };
        in
        naersk-lib.buildPackage {
          pname = "nix-olde";
          root = ./.;
        };
    in
    utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          packages = {
            default = self.packages."${system}".nix-olde;
            nix-olde = oldeLambda pkgs;
          };

          apps.default = utils.lib.mkApp {
            drv = self.packages."${system}".default;
          };

          devShells.default = with pkgs; mkShell {
            nativeBuildInputs = [ cargo cargo-edit rustc rustfmt rustPackages.clippy ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
        })
    // {
      overlays.default = (final: prev: {
        nix-olde = oldeLambda prev;
      });
    };
}
