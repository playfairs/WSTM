{
  description = "Development shell for wstm";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";

  outputs = { self, nixpkgs }: let
    pkgs_x86_64 = import nixpkgs { system = "x86_64-darwin"; };
    pkgs_aarch64 = import nixpkgs { system = "aarch64-darwin"; };
  in {
    devShells = {
      x86_64-darwin = {
        default = pkgs_x86_64.mkShell {
          buildInputs = [];
        };
      };
      aarch64-darwin = {
        default = pkgs_aarch64.mkShell {
          buildInputs = [];
        };
      };
    };
  };
}
