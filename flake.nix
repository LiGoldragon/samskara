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
    criome-cozo = { url = "github:LiGoldragon/criome-cozo"; flake = false; };
    samskara-core = { url = "github:LiGoldragon/samskara-core"; flake = false; };
    samskara-lojix-contract = { url = "github:LiGoldragon/samskara-lojix-contract"; flake = false; };
    samskara-codegen = { url = "github:LiGoldragon/samskara-codegen"; flake = false; };
  };

  outputs = { self, nixpkgs, flake-utils, crane, fenix, criome-cozo, samskara-core, samskara-lojix-contract, samskara-codegen, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rustToolchain = fenix.packages.${system}.latest.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Include .cozo and .capnp files alongside standard cargo sources
        extraFilter = path: _type: builtins.match ".*\\.(cozo|capnp)$" path != null;
        sourceFilter = path: type:
          (extraFilter path type) || (craneLib.filterCargoSources path type);
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = sourceFilter;
        };

        commonArgs = {
          inherit src;
          pname = "samskara";
          # Place path deps where Cargo.toml expects them (../<dep>)
          nativeBuildInputs = [ pkgs.capnproto ];
          postUnpack = ''
            depDir=$(dirname $sourceRoot)
            cp -rL ${criome-cozo} $depDir/criome-cozo
            cp -rL ${samskara-core} $depDir/samskara-core
            cp -rL ${samskara-lojix-contract} $depDir/samskara-lojix-contract
            cp -rL ${samskara-codegen} $depDir/samskara-codegen
          '';
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in
      {
        packages.default = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

        checks = {
          build = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
          tests = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [ rust-analyzer sqlite capnproto jujutsu ];
        };
      }
    );
}
