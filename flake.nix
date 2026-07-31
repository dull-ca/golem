{
  description = "golem build outputs: golem agent + CLI, emet compiler, and LSP";

  # Repo-scoped cachix cache (ADR 0035): applies only to builds of this flake.
  nixConfig = {
    extra-substituters = [ "https://dull-ca.cachix.org" ];
    extra-trusted-public-keys = [
      "dull-ca.cachix.org-1:dRCsbIU6rWu2X/4+BOxwvtyVOHUXXmRp7ZmEXwne9bk="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };
        pkgsStatic = pkgs.pkgsStatic;
        inherit (pkgs) lib;

        craneLibStatic = crane.mkLib pkgsStatic;

        # A positive allow-list rather than crane's `cleanCargoSource`, which
        # strips the non-`.rs` `.emet` fixtures the emet suites read. The payoff:
        # edits outside these roots (docs/, sites/) leave every rust build cached.
        rustSourceRoots = [
          "Cargo.toml"
          "Cargo.lock"
          "emet.json"
          "apps"
          "examples"
          "lib"
          "libs"
        ];

        rustSource =
          let repoRoot = toString ./.; in
          lib.cleanSourceWith {
            name = "golem-rust-source";
            src = ./.;
            filter = path: _type:
              let relative = lib.removePrefix "${repoRoot}/" (toString path);
              in lib.elem (lib.head (lib.splitString "/" relative)) rustSourceRoots;
          };

        commonArgs = {
          src = rustSource;
          version = "0.1.0";
          strictDeps = true;
        };

        # Every binary is a portable static-musl build, for Debian guests. A
        # nix-dynamic binary links its interpreter as a /nix/store path, so it
        # can't run off NixOS; pkgsStatic links everything (musl libc, bundled
        # sqlite, rustls/ring crypto) into one file. One target, so one deps
        # graph below and one set of outputs.
        #
        # crane derives CARGO_BUILD_TARGET and the cross linker/CC vars from
        # pkgsStatic's host platform; only `+crt-static` has to be stated.
        staticArgs = commonArgs // {
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
          doCheck = false;
        };

        # Third-party deps, compiled against dummied-out workspace sources: the
        # store path survives any `.rs` edit here, so it stays a cachix hit.
        cargoArtifacts = craneLibStatic.buildDepsOnly (staticArgs // {
          pname = "golem-workspace-static";
        });

        # The golem agent (golemd) and CLI (golemctl) as their own flake
        # outputs: each builds only its own workspace crate, off the shared
        # Cargo.lock. These are release outputs — CI builds them via the
        # `checks` below (`nix flake check`; ADR 0035).
        golemd = craneLibStatic.buildPackage (staticArgs // {
          pname = "golemd";
          inherit cargoArtifacts;
          # Setting cargoExtraArgs drops crane's own `--locked`, hence the
          # explicit one.
          cargoExtraArgs = "--locked -p golemd";
        });

        golemctl = craneLibStatic.buildPackage (staticArgs // {
          pname = "golemctl";
          inherit cargoArtifacts;
          cargoExtraArgs = "--locked -p golemctl";
        });

        emetc = craneLibStatic.buildPackage (staticArgs // {
          pname = "emet";
          inherit cargoArtifacts;
          cargoExtraArgs = "--locked -p emet";
        });

        emet-lsp = craneLibStatic.buildPackage (staticArgs // {
          pname = "emet-lsp";
          inherit cargoArtifacts;
          cargoExtraArgs = "--locked -p emet-lsp";
        });

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

        # `cargo test` over the whole workspace, in one derivation — including
        # cross-crate integration tests and the crates with no release binary
        # (e.g. scroll-format). It is the test half of the `nix flake check`
        # gate (ADR 0035 §1); the per-package builds are the build half, and
        # they carry `doCheck = false` because this owns testing.
        workspace-tests = craneLibStatic.cargoTest (staticArgs // {
          pname = "golem-workspace-tests";
          inherit cargoArtifacts;
          # Load-bearing: crane runs the tests in the *check* phase, so
          # inheriting staticArgs' `doCheck = false` would compile the test
          # binaries, run none of them, and still report the gate green.
          doCheck = true;
          # Nothing downstream consumes this check's target dir; crane's default
          # would push ~38 MiB of artifacts to cachix every run.
          doInstallCargoArtifacts = false;
        });

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
          inherit golemd golemctl emetc emet-lsp;
          # Aliases, not builds: every output is static now. devenv's
          # `build-static` and apps/fleet/deploy.py still name these.
          golemd-static = golemd;
          golemctl-static = golemctl;
          default = emetc;
        } // pkgs.lib.optionalAttrs (siteDist != null) {
          inherit website-container;
        };

        # The complete CI gate: `nix flake check` builds every one of these
        # (ADR 0035 §1). The four binary builds prove the toolchain compiles;
        # workspace-tests and fleet-tests prove it passes. `website-container` is
        # deliberately absent — it needs `--impure` + an external dist (ADR 0035
        # §4).
        checks = {
          inherit golemd golemctl emetc emet-lsp;
          inherit workspace-tests fleet-tests;
        };

        lib.mkWebsiteContainer = mkWebsiteContainer;
      });
}
