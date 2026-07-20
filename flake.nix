{
  description = "golem build outputs: emet compiler, LSP, and tree-sitter grammar";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };

        cargoLock = { lockFile = ./Cargo.lock; };

        emetc = pkgs.rustPlatform.buildRustPackage {
          pname = "emet";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
          cargoBuildFlags = [ "-p" "emet" ];
          cargoTestFlags = [ "-p" "emet" ];
        };

        emet-lsp = pkgs.rustPlatform.buildRustPackage {
          pname = "emet-lsp";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
          cargoBuildFlags = [ "-p" "emet-lsp" ];
          cargoTestFlags = [ "-p" "emet-lsp" ];
        };

        tree-sitter-emet = pkgs.tree-sitter.buildGrammar {
          language = "emet";
          version = "0.1.0";
          src = ./libs/tree-sitter-emet;
        };
      in
      {
        packages = {
          inherit emetc emet-lsp tree-sitter-emet;
          default = emetc;
        };
      });
}
