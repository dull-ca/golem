{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
  };

  # Docs site lives under sites/website — Astro + Starlight, runs on bun.
  packages = [
    pkgs.bun
    pkgs.nodejs_20
    pkgs.tree-sitter
  ];

  scripts.build.exec = "cargo build --workspace";
  scripts.test.exec = "cargo test --workspace";
  scripts.site.exec = "cd sites/website && bun run dev";
  scripts.build-site.exec = "cd sites/website && bun run build";
  scripts.build-all.exec = ''
    cargo build --workspace
    (cd libs/tree-sitter-emet && tree-sitter generate)
    (cd sites/website && bun run build)
  '';
}
