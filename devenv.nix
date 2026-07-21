{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
    targets = [ "x86_64-unknown-linux-musl" ];
  };

  # Docs site lives under sites/website — Astro + Starlight, runs on bun.
  packages = [
    pkgs.bun
    pkgs.nodejs_20
    pkgs.tree-sitter
    # qemu through zig: the fleet harness (apps/fleet). qemu boots the Debian
    # VMs; cloud-utils/xorriso/cdrkit build the cloud-init seed ISO; openssh and
    # curl reach and provision the guests; zig cross-links the static musl
    # golemd; python + typer/rich/httpx are the CLI itself.
    pkgs.qemu
    pkgs.cloud-utils
    pkgs.xorriso
    pkgs.cdrkit
    pkgs.openssh
    pkgs.curl
    pkgs.zig
    (pkgs.python3.withPackages (ps: with ps; [ typer rich httpx ]))
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
  # `fleet` runs the harness from the repo root with apps/ on PYTHONPATH: the cd
  # anchors relative `.emet` paths (and .fleet/) at the checkout root regardless
  # of the caller's cwd, and PYTHONPATH lets `python -m fleet` import apps/fleet.
  scripts.fleet.exec = ''cd "$DEVENV_ROOT" && PYTHONPATH="$DEVENV_ROOT/apps''${PYTHONPATH:+:$PYTHONPATH}" exec python -m fleet "$@"'';
}
