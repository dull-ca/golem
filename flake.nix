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
    # `buildBunPackage` / `fetchBunDeps` — the bun-in-nix machinery shared with
    # dull.yyc.dev, so both sites build the same way and a fix lands once.
    dull-nix.url = "github:dull-ca/nix";
  };

  outputs = { self, nixpkgs, flake-utils, crane, dull-nix }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ dull-nix.overlays.default ];
        };
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

        # The docs-owned example tree (ADR 0043). `apps/emet/tests/docs_examples.rs`
        # compiles every program in it and asserts its rendered output, so it is
        # rust test input even though it lives under `sites/`. Filtering it out
        # would leave the test unable to find a single example — which it fails
        # loudly on rather than skipping.
        docsExamplesRoot = "sites/website/examples";

        rustSource =
          let repoRoot = toString ./.; in
          lib.cleanSourceWith {
            name = "golem-rust-source";
            src = ./.;
            filter = path: _type:
              let
                relative = lib.removePrefix "${repoRoot}/" (toString path);
                isRustRoot = lib.elem (lib.head (lib.splitString "/" relative)) rustSourceRoots;
                # Both directions: the ancestors of the docs example tree have to
                # survive the filter for the tree itself to be reachable.
                isDocsExamplesAncestor = lib.hasPrefix "${relative}/" "${docsExamplesRoot}/";
                isInDocsExamples = lib.hasPrefix "${docsExamplesRoot}/" "${relative}/";
              in
              isRustRoot || isDocsExamplesAncestor || isInDocsExamples;
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

        # `packages.default`: a bare `nix build` leaves all four binaries under
        # ./result/bin, and `nix profile install .#golem-tools` puts them on
        # PATH outside this checkout — the only way emet-lsp reaches an editor
        # opened on a consumer repo. Install the attribute, not `.`: the profile
        # element takes its name from the flake reference, so `.` names it after
        # the checkout directory and `nix profile remove golem-tools` misses.
        golem-tools = pkgs.symlinkJoin {
          name = "golem-tools-${commonArgs.version}";
          paths = [ emetc emet-lsp golemd golemctl ];
          meta = {
            description =
              "Every golem binary in one output: emetc, emet-lsp, golemd, golemctl";
            # A joined output has no binary of its own, so `nix run .` must be
            # told which one it means; emetc is what it ran before the join.
            mainProgram = "emetc";
          };
        };

        # Everything the site build reads, minus the artifacts a build writes:
        # `dist/` and `.astro/` are outputs, not inputs, so an in-tree copy from
        # a `bun run build` must not change this derivation.
        websiteSrc = lib.cleanSourceWith {
          name = "golem-website-src";
          src = lib.cleanSource ./sites/website;
          filter = path: type:
            let rel = lib.removePrefix (toString ./sites/website + "/") (toString path);
            in !(lib.hasPrefix "node_modules" rel
              || lib.hasPrefix "dist" rel
              || lib.hasPrefix ".astro" rel);
        };

        # Bump when sites/website/bun.lock or package.json changes; nix reports
        # the replacement on mismatch (see dull-nix's fetchBunDeps).
        websiteBunDepsHash = "sha256-XY0AEsZ2vmcL6OmVSvUoGCFX4MCXKwhyXd3Ly/pLJ/E=";

        # The built docs site. `buildBunPackage` owns what used to be spelled out
        # here by hand: the fixed-output `bun install`, `autoPatchelfHook` over
        # npm's prebuilt ELF binaries, and `patchShebangs` over their
        # `#!/usr/bin/env node` entry points.
        websiteDist = pkgs.buildBunPackage {
          pname = "golem-website-dist";
          src = websiteSrc;
          bunDepsHash = websiteBunDepsHash;
        };

        # NOTE: `dull-nix.packages`, not the overlay — overlays.default carries
        # only the bun builders. The binary is static, TLS-less nginx; see
        # docs/adr/0052-nginx-static-docs-image.md for why the docs image runs
        # it rather than caddy.
        websiteNginx = dull-nix.packages.${system}.nginx-static-no-tls;

        # The docs site as an nginx image (see sites/website/nginx.conf), built
        # from `websiteDist` above — purely, with no external `dist/` and no
        # `--impure`.
        mkWebsiteContainer = dist: pkgs.dockerTools.buildLayeredImage {
          name = "golem-website";
          tag = "latest";
          contents = [
            websiteNginx
            # Supplies /etc/passwd and /etc/group so the `nobody` below
            # resolves — the image is built from an empty base with no distro
            # files, and nginx refuses to start when its user does not exist.
            pkgs.dockerTools.fakeNss
            (pkgs.runCommand "golem-website-root" { } ''
              mkdir -p $out/var/www/html $out/etc/nginx
              cp -r ${dist}/. $out/var/www/html/
              cp ${./sites/website/nginx.conf} $out/etc/nginx/nginx.conf
              cp ${websiteNginx}/conf/mime.types $out/etc/nginx/mime.types
            '')
          ];
          # NOTE: a `chmod 1777` inside the runCommand above would not survive —
          # nix's store canonicalisation resets directory modes to 0555.
          # extraCommands runs against the image layer after that, so it is the
          # only place the bit sticks. nginx needs a writable /tmp for its pid
          # and client-body files.
          extraCommands = ''
            mkdir -p tmp
            chmod 1777 tmp
          '';
          config = {
            Entrypoint = [ "/bin/nginx" "-c" "/etc/nginx/nginx.conf" ];
            ExposedPorts = { "8080/tcp" = { }; };
            User = "nobody";
            # Travels in the image manifest, so `skopeo inspect` surfaces both
            # without reading an ADR. `image.source` is also what links the
            # published ghcr.io package back to this repository.
            Labels = {
              "org.opencontainers.image.description" =
                "golem's documentation site. Serves plaintext HTTP on :8080. "
                + "nginx is built without ngx_http_ssl_module and CANNOT serve "
                + "HTTPS — it must sit behind a TLS-terminating reverse proxy "
                + "and must never be exposed directly to the internet.";
              "org.opencontainers.image.source" = "https://github.com/dull-ca/golem";
            };
          };
        };

        website-container = mkWebsiteContainer websiteDist;

        # Does the site actually serve? `nix flake check` used to prove only
        # that the flake evaluated; this runs the real nginx against the real
        # `nginx.conf` and the real built site, and asserts what a reader gets.
        #
        # Three substitutions, all forced by the sandbox and none touching
        # behaviour: the document root and the `mime.types` include move off
        # `/var/www/html` and `/etc/nginx`, which a build cannot create, and the
        # pid file moves off `/tmp`. The listen port needs no rewrite — it is
        # already unprivileged. Every other directive — the gzip settings,
        # `charset utf-8`, `absolute_redirect off`, the `error_page` fallback to
        # Starlight's 404.html — is the file as shipped, which is where the
        # behaviour under test lives.
        #
        # The four assertions are the four things a reader would notice
        # breaking, and each one caught a real divergence while the server was
        # being swapped (docs/adr/0052-nginx-static-docs-image.md). The 404 is
        # asserted twice over — status AND body — because serving the styled
        # page with a 200 would look right in a browser and be wrong to every
        # crawler.
        website-serves = pkgs.runCommand "golem-website-serves"
          {
            nativeBuildInputs = [ pkgs.curl ];
          } ''
          substitute ${./sites/website/nginx.conf} nginx.conf \
            --replace-fail '/etc/nginx/mime.types' '${websiteNginx}/conf/mime.types' \
            --replace-fail '/var/www/html' '${websiteDist}' \
            --replace-fail '/tmp/nginx.pid' "$PWD/nginx.pid"
          ${websiteNginx}/bin/nginx -c $PWD/nginx.conf -p $PWD &
          nginx_pid=$!
          trap 'kill $nginx_pid 2>/dev/null || true' EXIT

          for _ in $(seq 1 60); do
            curl -sf -o /dev/null http://127.0.0.1:8080/ && break
            sleep 0.5
          done

          root=$(curl -s -o index.html -w '%{http_code}' http://127.0.0.1:8080/)
          ctype=$(curl -s -o /dev/null -w '%{content_type}' http://127.0.0.1:8080/)
          missing=$(curl -s -o missing-body.html -w '%{http_code}' http://127.0.0.1:8080/definitely-not-here)

          test "$root" = 200 || { echo "FAIL root: $root"; exit 1; }
          test "$ctype" = "text/html; charset=utf-8" || { echo "FAIL content-type: $ctype"; exit 1; }
          test "$missing" = 404 || { echo "FAIL 404: $missing"; exit 1; }
          grep -qF '<title>404 | Golem</title>' missing-body.html \
            || { echo "FAIL styled 404 body"; head -c 400 missing-body.html; exit 1; }

          stylesheet=$(grep -o '/_astro/[^"]*\.css' index.html | head -1)
          test -n "$stylesheet" || { echo "FAIL no stylesheet in index.html"; exit 1; }
          curl -s -o /dev/null -D asset-headers.txt -H 'Accept-Encoding: gzip' \
            "http://127.0.0.1:8080$stylesheet"
          grep -qi '^content-type: text/css; charset=utf-8' asset-headers.txt \
            || { echo "FAIL stylesheet content-type"; cat asset-headers.txt; exit 1; }
          grep -qi '^content-encoding: gzip' asset-headers.txt \
            || { echo "FAIL stylesheet not compressed"; cat asset-headers.txt; exit 1; }

          directory=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' \
            http://127.0.0.1:8080/getting-started/install)
          test "$directory" = "301 http://127.0.0.1:8080/getting-started/install/" \
            || { echo "FAIL directory redirect: $directory"; exit 1; }

          echo "the site serves" > $out
        '';

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
          inherit golemd golemctl emetc emet-lsp golem-tools;
          # Aliases, not builds: every output is static now. devenv's
          # `build-static` and apps/fleet/deploy.py still name these.
          golemd-static = golemd;
          golemctl-static = golemctl;
          default = golem-tools;
          inherit websiteDist website-container;
        };

        # The complete CI gate: `nix flake check` builds every one of these
        # (ADR 0035 §1). The four binary builds prove the toolchain compiles;
        # workspace-tests and fleet-tests prove it passes. `website-container`
        # is absent because `websiteDist` and `website-serves` already cover
        # what it wraps; the image itself is built on a tag, by release.yml.
        checks = {
          inherit golemd golemctl emetc emet-lsp golem-tools;
          inherit workspace-tests fleet-tests;
          # The docs site is part of the gate now that it builds purely:
          # `websiteDist` proves it compiles, `website-serves` proves it serves.
          inherit websiteDist website-serves;
        };

        lib.mkWebsiteContainer = mkWebsiteContainer;
      });
}
