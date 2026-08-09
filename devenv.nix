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
    # `cachix.enable` above configures the cache but ships no binary, and
    # `warm-cache` below needs one. `gh` is for the release script; both were
    # reachable only from Dr. Dub's own NixOS profile before.
    pkgs.cachix
    pkgs.gh
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
  # The gate CI runs — the same `nix flake check` over the same `checks`
  # attrset, which is the only definition of it — with every path it builds
  # pushed to cachix, so the CI run afterwards has nothing left to build.
  #
  # NOTE: `cachix watch-exec` prints a red ✗ for a rejected push and still
  # exits 0, hence the check that the outputs really landed. Nix caches a
  # "not in this cache" answer for an hour, hence the zeroed negative TTL.
  scripts.warm-cache.exec = ''
    set -euo pipefail
    cd "$DEVENV_ROOT"
    cachixConfig="''${XDG_CONFIG_HOME:-$HOME/.config}/cachix/cachix.dhall"
    if [ -z "''${CACHIX_AUTH_TOKEN:-}" ] && [ ! -f "$cachixConfig" ]; then
      {
        echo "warm-cache: no cachix auth token — every push would silently no-op."
        echo
        echo "Mint a write token for the dull-ca cache at"
        echo "    https://app.cachix.org/cache/dull-ca/settings/authtokens"
        echo "then store it once:"
        echo
        echo "    cachix authtoken <token>"
        echo "    (writes $cachixConfig)"
        echo
        echo "or export it for this shell only (nushell):"
        echo
        echo '    $env.CACHIX_AUTH_TOKEN = "<token>"'
      } >&2
      exit 1
    fi
    cachix watch-exec dull-ca -- nix flake check --print-build-logs

    gateOutputs=$(nix eval --raw '.#checks.x86_64-linux' --apply \
      'checks: builtins.concatStringsSep "\n" (map (c: c.outPath) (builtins.attrValues checks))')

    unpushed=""
    for path in $gateOutputs; do
      if ! nix path-info --store https://dull-ca.cachix.org \
        --narinfo-cache-negative-ttl 0 "$path" >/dev/null 2>&1; then
        unpushed="$unpushed  $path"$'\n'
      fi
    done

    if [ -n "$unpushed" ]; then
      {
        echo "warm-cache: the gate passed, but these outputs never reached dull-ca:"
        printf '%s' "$unpushed"
        echo "cachix marks a rejected push with a red x and still exits 0, so scroll up."
        echo "The usual cause is a token with no write access to dull-ca."
      } >&2
      exit 1
    fi

    echo "warm-cache: gate passed, every output is in dull-ca — CI has nothing left to build."
  '';
  # `fleet` runs the harness from the repo root with apps/ on PYTHONPATH: the cd
  # anchors relative `.emet` paths (and .fleet/) at the checkout root regardless
  # of the caller's cwd, and PYTHONPATH lets `python -m fleet` import apps/fleet.
  scripts.fleet.exec = ''cd "$DEVENV_ROOT" && PYTHONPATH="$DEVENV_ROOT/apps''${PYTHONPATH:+:$PYTHONPATH}" exec python -m fleet "$@"'';
}
