{
  description = "Chess.com game downloader";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs = {nixpkgs, ...}: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forAllSystems = nixpkgs.lib.genAttrs systems;
  in {
    packages = forAllSystems (system: {
      default = nixpkgs.legacyPackages.${system}.callPackage ./nix/default.nix {};
    });

    devShells = forAllSystems (system: {
      default = nixpkgs.legacyPackages.${system}.callPackage ./nix/shell.nix {};
    });
  };
}
