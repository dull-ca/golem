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
        # shared Cargo.lock. CI (`.woodpecker.yml`) builds `.#golemd .#golemctl`
        # as release binaries.
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
        # (which only sees committed source) can't read it. Instead `GOLEM_SITE_DIST`
        # carries the built path; reading an env var forces `nix build --impure`.
        # Unset (e.g. a local `dist/` present in the source tree) falls back to
        # the in-tree path.
        siteDist =
          let env = builtins.getEnv "GOLEM_SITE_DIST";
          in
          if env != "" then /. + env
          else if builtins.pathExists ./sites/website/dist then ./sites/website/dist
          else null;

        # The default website image, wired to the env-supplied dist path above.
        # Built (not pushed) in CI as `nix build --impure .#website-container`.
        website-container = mkWebsiteContainer siteDist;

        workspace-tests = pkgs.rustPlatform.buildRustPackage {
          pname = "golem-workspace-tests";
          version = "0.1.0";
          src = ./.;
          inherit cargoLock;
        };

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

        checks = {
          inherit golemd golemctl emetc emet-lsp golemd-static golemctl-static;
          inherit workspace-tests fleet-tests;
        };

        lib.mkWebsiteContainer = mkWebsiteContainer;
      });
}
