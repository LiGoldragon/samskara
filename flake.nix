{
  description = "Samskara — pure datalog agent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    criome-cozo-src = { url = "github:LiGoldragon/criome-cozo"; flake = false; };
    samskara-lojix-contract-src = { url = "github:LiGoldragon/samskara-lojix-contract"; flake = false; };
  };

  outputs = inputs@{ self, nixpkgs, flake-utils, crane, fenix, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rustToolchain = fenix.packages.${system}.latest.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        src = craneLib.cleanCargoSource ./.;
      in
      {
        packages.default = craneLib.buildPackage {
          inherit src;
          pname = "samskara";
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [ rust-analyzer sqlite ];
        };
      }
    );
}
