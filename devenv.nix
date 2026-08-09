{ pkgs, ... }:

{
  # Caches the devenv environment itself; golem build artifacts flow through
  # the flake's nixConfig + cachix watch-exec instead.
  cachix.enable = true;
  cachix.pull = [ "dull-ca" ];
  cachix.push = "dull-ca";

  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
  };

  # Docs site lives under sites/website — Astro + Starlight, runs on bun.
  packages = [
    pkgs.bun
    pkgs.nodejs_20
    # The fleet harness (apps/fleet). qemu boots the Debian VMs;
    # cloud-utils/xorriso/cdrkit build the cloud-init seed ISO; openssh and curl
    # reach and provision the guests; the static musl golemd comes from the
    # `golemd-static` flake output; python + typer/rich/httpx are the CLI itself.
    pkgs.qemu
    pkgs.cloud-utils
    pkgs.xorriso
    pkgs.cdrkit
    pkgs.openssh
    pkgs.curl
    (pkgs.python3.withPackages (ps: with ps; [ typer rich httpx ]))
  ];

  # Freshly built workspace binaries (emet-lsp for nvim, emetc, golemctl…)
  # win over any installed copies while inside the shell.
  enterShell = ''
    export PATH="$DEVENV_ROOT/target/release:$PATH"
  '';

  scripts.build.exec = "cargo build --workspace";
  scripts.build-static.exec = "nix build .#golemd-static .#golemctl-static --print-build-logs";
  scripts.test.exec = "cargo test --workspace";
  scripts.site.exec = "cd sites/website && bun run dev";
  scripts.build-site.exec = "cd sites/website && bun run build";
  # Named --out-links: a second `nix build` would otherwise land on `result-1`.
  scripts.build-all.exec = ''
    cd "$DEVENV_ROOT"
    nix build --print-build-logs --out-link result
    nix build .#websiteDist --print-build-logs --out-link result-site
    echo "binaries: $DEVENV_ROOT/result/bin"
    echo "site:     $DEVENV_ROOT/result-site"
  '';
  # Reinstall, not install: `nix profile install` errors on an element already
  # there, and reinstalling is how the profile picks up new commits.
  scripts.install-tools.exec = ''
    nix profile remove golem-tools >/dev/null 2>&1 || true
    nix profile install "$DEVENV_ROOT#golem-tools"
    echo "golem-tools installed: emetc, emet-lsp, golemd, golemctl are on PATH outside this checkout"
  '';
  scripts.uninstall-tools.exec = ''nix profile remove golem-tools'';
  # `fleet` runs the harness from the repo root with apps/ on PYTHONPATH: the cd
  # anchors relative `.emet` paths (and .fleet/) at the checkout root regardless
  # of the caller's cwd, and PYTHONPATH lets `python -m fleet` import apps/fleet.
  scripts.fleet.exec = ''cd "$DEVENV_ROOT" && PYTHONPATH="$DEVENV_ROOT/apps''${PYTHONPATH:+:$PYTHONPATH}" exec python -m fleet "$@"'';
}
