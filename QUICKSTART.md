# Quickstart

> **Status today (M1):** the layer-3 engine (File / AptPackage / SystemdUnit providers, signed bundles, journal-before-mutate, orphan sweep) is the part that's working. The Nickel-driven `golemctl apply` flow described below is the M2 target — the existing `examples/simple/config.ncl` does not yet evaluate cleanly against current Nickel (see REVIEW.md §2 for the Nickel-stdlib issues to fix). For an end-to-end M1 demo, use the hand-written JSON bundles in `smoke-test/` and the `golemctl sign` path described under "Single-node test" below. The full flow described next is what M2 will deliver.

Build static binaries:

    ./build-static.sh

Generate an operator keypair:

    ./target/x86_64-unknown-linux-musl/release/golemctl keygen ./operator

This creates `operator.sk` (private, mode 0600) and `operator.pk` (public).

Distribute the public key to every node — it becomes their trusted root:

    sudo install -d -m 0755 /etc/golem
    sudo cp operator.pk /etc/golem/trusted-keys

Install the agent on each node:

    sudo install -m 0755 target/x86_64-unknown-linux-musl/release/golemd /usr/local/bin/
    sudo install -m 0644 packaging/golemd.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now golemd

Apply the example config (from your laptop):

    cd examples/simple
    golemctl apply config.ncl ../../operator.sk node-addrs.json

What happens on each node:

  1. golemctl runs `nickel export` to evaluate `config.ncl` for that node.
  2. The bundle is signed with `operator.sk`.
  3. The signed bundle is POSTed to `http://<addr>:7474/bundle`.
  4. The agent verifies the signature against `/etc/golem/trusted-keys`,
     unpacks Quadlet claims into File + SystemdUnit pairs, dedupes by
     ClaimId, and swaps the in-memory desired set.
  5. On the next reconcile tick (≤30s), the agent installs podman / caddy,
     drops the .container file, daemon-reloads, and starts the units.

Single-node test on your dev box (no remote):

    # M1-ready: hand-write a bundle, sign it, and run the agent against it.
    # No Nickel involved — the smoke-test directory has working examples.
    golemctl sign smoke-test/bundle-v1-install.json operator.sk > /tmp/signed.json

    # Run the agent in the foreground, loading from disk
    sudo ./target/release/golemd \
        --node test-01 \
        --state-dir /tmp/golem-state \
        --trusted-keys ./operator.pk \
        --bundle /tmp/signed.json \
        --listen 127.0.0.1:7474

    # Once Nickel translation lands (M2), the `golemctl eval` form will work:
    #   golemctl eval examples/simple/config.ncl app-01 > /tmp/bundle.json

Inspecting state:

    curl -s http://127.0.0.1:7474/status | jq
    sqlite3 /var/lib/golem/state.db 'SELECT id_kind, id_key FROM claim_state;'

Containerized integration test (no host system pollution):

    docker build -t golem-smoke:trixie crates/golemd/tests/fixtures
    cargo build --release -p golemd
    cargo test -p golemd --test smoke_install_remove --release -- --ignored --nocapture

This brings up a fresh `debian:trixie + systemd` container, installs and removes
caddy through the agent, and asserts the journal cleans up to zero rows. ~20s
end-to-end. See `smoke-test/run.sh` for the bash equivalent including the four
`GOLEM_CRASH_AFTER` injection points that exercise crash recovery.
