{
  description = "golem build outputs: golem agent + CLI, emet compiler, and LSP";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };
        pkgsStatic = pkgs.pkgsStatic;

        cargoLock = { lockFile = ./Cargo.lock; };

        # The golem agent (golemd) and CLI (golemctl) as their own flake
        # outputs: each builds and tests only its own workspace crate, off the
        # shared Cargo.lock. These are release outputs — CI builds them via the
        # `checks` below (`nix flake check`; ADR 0035).
        golemd = pkgs.rustPlatform.buildRustPackage {
          pname = "golemd";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
          cargoBuildFlags = [ "-p" "golemd" ];
          cargoTestFlags = [ "-p" "golemd" ];
        };

        golemctl = pkgs.rustPlatform.buildRustPackage {
          pname = "golemctl";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
          cargoBuildFlags = [ "-p" "golemctl" ];
          cargoTestFlags = [ "-p" "golemctl" ];
        };

        # Portable static-musl golemd/golemctl for Debian guests. A nix-dynamic
        # binary links its interpreter as a /nix/store path, so it can't run off
        # NixOS; pkgsStatic links everything (musl libc, bundled sqlite,
        # rustls/ring crypto) into one file. `nix build .#golemd-static`.
        golemd-static = pkgsStatic.rustPlatform.buildRustPackage {
          pname = "golemd-static";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
          cargoBuildFlags = [ "-p" "golemd" ];
          doCheck = false;
        };

        golemctl-static = pkgsStatic.rustPlatform.buildRustPackage {
          pname = "golemctl-static";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
          cargoBuildFlags = [ "-p" "golemctl" ];
          doCheck = false;
        };

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

        # Package a pre-built Starlight `dist/` into a Caddy image (see
        # sites/website/Caddyfile). The site is built bun-first, then packaged
        # here, rather than built purely in Nix: Astro/Starlight pulls ~137
        # platform-split native packages (sharp/libvips, esbuild, rollup,
        # pagefind), which buildNpmPackage can't reproduce reliably. So CI runs
        # `bun run build` and this only copies the resulting path into the image.
        mkWebsiteContainer = dist: pkgs.dockerTools.buildLayeredImage {
          name = "golem-website";
          tag = "latest";
          contents = [
            pkgs.caddy
            (pkgs.runCommand "golem-website-root" { } ''
              mkdir -p $out/var/www/html $out/etc/caddy
              cp -r ${dist}/. $out/var/www/html/
              cp ${./sites/website/Caddyfile} $out/etc/caddy/Caddyfile
            '')
          ];
          config = {
            Entrypoint = [
              "${pkgs.caddy}/bin/caddy"
              "run"
              "--config"
              "/etc/caddy/Caddyfile"
              "--adapter"
              "caddyfile"
            ];
            ExposedPorts = { "80/tcp" = { }; };
          };
        };

        # `dist/` is a gitignored build artifact, so a pure flake evaluation
        # (which only sees committed source) can't read it. Three cases, in order:
        # `GOLEM_SITE_DIST` carries the built path (reading the env var forces
        # `nix build --impure`); else an in-tree `dist/` if one is present; else
        # null. Null means no dist is available, and `website-container` is then
        # omitted from `packages` entirely (below) rather than failing eval — so a
        # pure `nix flake check`, which walks every `packages` output, stays green.
        siteDist =
          let env = builtins.getEnv "GOLEM_SITE_DIST";
          in
          if env != "" then /. + env
          else if builtins.pathExists ./sites/website/dist then ./sites/website/dist
          else null;

        # The default website image, wired to the dist path above. Present in
        # `packages` only when `siteDist != null` (the `optionalAttrs` below);
        # `let` is lazy, so this binding is never forced when siteDist is null.
        # Built (not pushed) as `nix build --impure .#website-container` — outside
        # the `checks` gate, since Astro's dist can't be produced purely (ADR 0035
        # §4).
        website-container = mkWebsiteContainer siteDist;

        # `cargo test` over the whole workspace, in one derivation. The
        # per-package outputs above run only their own crate's tests (`-p`), so
        # nothing exercises cross-crate integration tests or the crates with no
        # release binary (e.g. scroll-format) — this closes that gap. It is the
        # test half of the `nix flake check` gate (ADR 0035 §1); the per-package
        # builds are the build half.
        workspace-tests = pkgs.rustPlatform.buildRustPackage {
          pname = "golem-workspace-tests";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
        };

        # The `apps/fleet` python harness (fleet is a python CLI, not a Rust
        # crate), run under `unittest` against a nix-built interpreter with its
        # deps.
        #
        # The `| cat` is load-bearing, not filler. `apps/fleet/cli.py` builds a
        # module-level `rich.Console()` with default tty auto-detection, and one
        # test asserts plain-text output. Under a bare `nix build`, the builder's
        # stdout goes straight to the build log, which `rich` reads as a tty — so
        # it colorizes and the plain-text assertion breaks. Piping through `cat`
        # forces a real pipe, `isatty()` reports false, and the output matches the
        # non-nix `pytest` run. stdenv's `set -o pipefail` still fails the build on
        # a real test failure, so the pipe hides nothing. Delete it and the gate
        # goes flaky under nix.
        fleet-tests = pkgs.runCommand "golem-fleet-tests"
          {
            nativeBuildInputs = [
              (pkgs.python3.withPackages (ps: with ps; [ typer rich httpx ]))
            ];
          } ''
          PYTHONPATH=${./.}/apps python -m unittest discover -s ${./.}/apps/fleet/tests | cat
          touch $out
        '';
      in
      {
        packages = {
          inherit golemd golemctl golemd-static golemctl-static emetc emet-lsp;
          default = emetc;
        } // pkgs.lib.optionalAttrs (siteDist != null) {
          inherit website-container;
        };

        # The complete CI gate: `nix flake check` builds every one of these
        # (ADR 0035 §1). The six binary builds prove the toolchain compiles;
        # workspace-tests and fleet-tests prove it passes. `website-container` is
        # deliberately absent — it needs `--impure` + an external dist (ADR 0035
        # §4).
        checks = {
          inherit golemd golemctl emetc emet-lsp golemd-static golemctl-static;
          inherit workspace-tests fleet-tests;
        };

        lib.mkWebsiteContainer = mkWebsiteContainer;
      });
}
