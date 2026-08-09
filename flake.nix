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
    # `buildBunPackage` and `nginx-static-no-tls`, shared with dull.yyc.dev so a
    # fix to either lands once.
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

        # An allow-list rather than crane's `cleanCargoSource`, which strips the
        # `.emet` fixtures the emet suites read. Edits outside these roots leave
        # every rust build cached.
        rustSourceRoots = [
          "Cargo.toml"
          "Cargo.lock"
          "emet.json"
          "apps"
          "examples"
          "lib"
          "libs"
        ];

        # Rust test input despite living under `sites/`:
        # `apps/emet/tests/docs_examples.rs` compiles every program here
        # (docs/adr/0043-docs-examples-are-real-compiled-code.md).
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

        # Every binary is static-musl because it has to run on Debian guests: a
        # nix-dynamic binary links its interpreter as a /nix/store path. crane
        # derives CARGO_BUILD_TARGET and the cross linker/CC vars from
        # pkgsStatic; only `+crt-static` has to be stated.
        staticArgs = commonArgs // {
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
          doCheck = false;
        };

        # Third-party deps against dummied-out workspace sources, so the store
        # path survives any `.rs` edit here and stays a cachix hit.
        cargoArtifacts = craneLibStatic.buildDepsOnly (staticArgs // {
          pname = "golem-workspace-static";
        });

        golemd = craneLibStatic.buildPackage (staticArgs // {
          pname = "golemd";
          inherit cargoArtifacts;
          # NOTE: setting cargoExtraArgs drops crane's own `--locked`, hence the
          # explicit one here and below.
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

        websiteDist = pkgs.buildBunPackage {
          pname = "golem-website-dist";
          src = websiteSrc;
          bunDepsHash = websiteBunDepsHash;
        };

        # NOTE: `dull-nix.packages`, not the overlay — overlays.default carries
        # only the bun builders.
        websiteNginx = dull-nix.packages.${system}.nginx-static-no-tls;

        # The published docs image (docs/adr/0052-nginx-static-docs-image.md).
        # `sites/website/nginx.conf` is shared verbatim with the tutorials'
        # podman-built image in `sites/website/Containerfile`.
        mkWebsiteContainer = dist: pkgs.dockerTools.buildLayeredImage {
          name = "golem-website";
          tag = "latest";
          contents = [
            websiteNginx
            # /etc/passwd and /etc/group, absent from an empty base: nginx
            # refuses to start when the `nobody` below does not resolve.
            pkgs.dockerTools.fakeNss
            (pkgs.runCommand "golem-website-root" { } ''
              mkdir -p $out/var/www/html $out/etc/nginx
              cp -r ${dist}/. $out/var/www/html/
              cp ${./sites/website/nginx.conf} $out/etc/nginx/nginx.conf
              cp ${websiteNginx}/conf/mime.types $out/etc/nginx/mime.types
            '')
          ];
          # nginx needs a writable /tmp for its pid and client-body files.
          # NOTE: a `chmod 1777` inside the runCommand above would not survive
          # nix's store canonicalisation, which resets directory modes to 0555;
          # extraCommands runs after it, so it is the only place the bit sticks.
          extraCommands = ''
            mkdir -p tmp
            chmod 1777 tmp
          '';
          config = {
            Entrypoint = [ "/bin/nginx" "-c" "/etc/nginx/nginx.conf" ];
            ExposedPorts = { "8080/tcp" = { }; };
            User = "nobody";
            # Travels in the image manifest, so `skopeo inspect` surfaces the
            # no-TLS warning without reading an ADR, and ghcr.io links the
            # published package back to this repository.
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

        # The real nginx against the shipped `nginx.conf` and the real built
        # site, asserting what a reader gets
        # (docs/adr/0052-nginx-static-docs-image.md).
        #
        # The three substitutions are forced by the sandbox and touch no
        # behaviour: `/var/www/html` and `/etc/nginx` are paths a build cannot
        # create, and `/tmp` is not writable here. Every other directive is the
        # file as shipped, which is where the behaviour under test lives.
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

        # The test half of the gate, so the per-package builds above carry
        # `doCheck = false`. Covers the crates with no release binary (e.g.
        # scroll-format) and the cross-crate integration tests.
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

        # `apps/fleet` is a python CLI, not a Rust crate, so it needs its own
        # check.
        #
        # NOTE: the `| cat` is load-bearing. `apps/fleet/cli.py` builds a
        # module-level `rich.Console()` with tty auto-detection, and one test
        # asserts plain-text output; under a bare `nix build` the builder's
        # stdout is the build log, which `rich` reads as a tty and colorizes.
        # The pipe makes `isatty()` false. `set -o pipefail` still fails the
        # build on a real failure, so it hides nothing.
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
        # (ADR 0035 §1). `website-container` is here so every push warms it in
        # cachix — left out, only a `v*` tag ever built the image, and the tag
        # paid for it. It costs the gate almost nothing: `websiteDist` and
        # `website-serves` already force the built site and the static nginx
        # into the closure, leaving the image itself about a second of tar.
        checks = {
          inherit golemd golemctl emetc emet-lsp golem-tools;
          inherit workspace-tests fleet-tests;
          inherit websiteDist website-serves website-container;
        };

        lib.mkWebsiteContainer = mkWebsiteContainer;
      });
}
