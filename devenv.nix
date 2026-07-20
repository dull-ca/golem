{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
  };

  # Docs site lives under docs/ — Astro + Starlight, runs on bun.
  packages = [
    pkgs.bun
    pkgs.nodejs_20
    pkgs.tree-sitter
  ];
}
