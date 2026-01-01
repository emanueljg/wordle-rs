let
  pkgs = import (import ./sources.nix).nixpkgs { };
in
pkgs.mkShell {
  packages = [
    pkgs.cargo
    pkgs.rustc
  ];
}
