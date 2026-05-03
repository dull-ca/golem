#!/usr/bin/env bash
# M1 smoke test — exercises the layer-3 engine end-to-end and verifies
# crash recovery at every journal-before-mutate injection point.
#
# Test 1 — clean install + remove cycle (no crash).
# Test 2 — for each crash window, abort mid-tick and verify the next run
#          converges honestly. Windows:
#            capture_persisted     — after capture journaled, before mutate
#            intent_applied_false  — after intent journaled, before mutate
#            mutate_completed      — after mutate succeeded, before final journal
#            intent_applied_true   — after final journal entry
#
# Requires: sudo, apt-get-able caddy package, systemctl present.
# Run on a Debian trixie box that does NOT already have caddy installed
# (the smoke test does its own install/remove cycles).

set -euo pipefail
cd "$(dirname "$0")"

GOLEMCTL="${GOLEMCTL:-../target/release/golemctl}"
GOLEMD="${GOLEMD:-../target/release/golemd}"
STATE_DIR="${STATE_DIR:-/tmp/golem-smoke-state}"
LISTEN="${LISTEN:-127.0.0.1:7474}"
PERIOD_SECS="${PERIOD_SECS:-5}"     # tight loop for fast smoke; prod is 30
TICK_WAIT_SECS="$((PERIOD_SECS * 4 + 5))"  # generous: ~4 ticks for convergence

if [[ ! -x "$GOLEMCTL" || ! -x "$GOLEMD" ]]; then
    echo "build first: cargo build --release -p golemctl -p golemd" >&2
    exit 2
fi

if [[ ! -f operator.sk ]]; then
    "$GOLEMCTL" keygen ./operator
fi

"$GOLEMCTL" sign bundle-v1-install.json operator.sk > signed-v1.json
"$GOLEMCTL" sign bundle-v2-remove.json  operator.sk > signed-v2.json

# ─── helpers ──────────────────────────────────────────────────────────────

cleanup_box() {
    sudo systemctl stop caddy.service 2>/dev/null || true
    sudo apt-get autoremove -y --purge caddy 2>/dev/null || true
    sudo rm -f /etc/caddy/Caddyfile
    sudo rm -rf "$STATE_DIR"
    mkdir -p "$STATE_DIR"
}

start_golemd() {
    # $1 — optional GOLEM_CRASH_AFTER value
    local crash_var="${1:-}"
    sudo -E env "GOLEM_CRASH_AFTER=$crash_var" \
        "$GOLEMD" \
        --node test-01 \
        --state-dir "$STATE_DIR" \
        --trusted-keys ./operator.pk \
        --bundle ./signed-v1.json \
        --listen "$LISTEN" \
        --period-secs "$PERIOD_SECS" &
    echo $!
}

stop_golemd() {
    local pid="$1"
    sudo kill -TERM "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

verify_caddy_installed() {
    systemctl is-active caddy.service >/dev/null
    test -f /etc/caddy/Caddyfile
    grep -q "hello from golem" /etc/caddy/Caddyfile
    dpkg-query -W -f='${Status}' caddy 2>/dev/null | grep -q "install ok installed"
}

verify_caddy_removed() {
    ! systemctl is-active caddy.service >/dev/null 2>&1
    ! test -f /etc/caddy/Caddyfile
    ! dpkg-query -W -f='${Status}' caddy 2>/dev/null | grep -q "install ok installed"
}

verify_journal_empty() {
    # No claim_state rows means the orphan sweep cleaned up every id.
    local count
    count=$(sudo sqlite3 "$STATE_DIR/state.db" \
        "SELECT count(*) FROM claim_state;" 2>/dev/null || echo "?")
    if [[ "$count" != "0" ]]; then
        echo "journal not empty after orphan sweep (rows=$count)" >&2
        sudo sqlite3 "$STATE_DIR/state.db" \
            "SELECT id_kind, id_key FROM claim_state;" >&2
        return 1
    fi
}

push_remove_bundle() {
    curl -fsS -XPOST -H "content-type: application/json" \
        --data-binary @signed-v2.json "http://${LISTEN}/bundle" >/dev/null
}

# ─── Test 1: clean install + remove ───────────────────────────────────────

echo "=== Test 1: clean install + remove ==="
cleanup_box
PID=$(start_golemd)
sleep "$TICK_WAIT_SECS"
verify_caddy_installed
echo "  install: ok"
push_remove_bundle
sleep "$TICK_WAIT_SECS"
verify_caddy_removed
verify_journal_empty
echo "  remove + sweep: ok"
stop_golemd "$PID"

# ─── Test 2: crash injection at every window ──────────────────────────────

for crash_pt in capture_persisted intent_applied_false mutate_completed intent_applied_true; do
    echo "=== Test 2: crash at $crash_pt ==="
    cleanup_box

    # Run with crash injection. The agent will SIGABRT mid-tick.
    PID=$(start_golemd "$crash_pt")
    if wait "$PID"; then
        echo "FAIL: agent did NOT abort with GOLEM_CRASH_AFTER=$crash_pt" >&2
        exit 1
    fi
    echo "  aborted as expected"

    # Restart without injection — must converge to fully-installed state.
    PID=$(start_golemd)
    sleep "$TICK_WAIT_SECS"
    verify_caddy_installed
    echo "  converged after restart"

    # Push remove bundle, verify orphan sweep restores honest state.
    # Especially important: if a previous crash left captured=true with
    # preexisting=true wrongly recorded, unmutate would skip the apt remove
    # and the dpkg check would still pass below — we'd catch that bug here.
    push_remove_bundle
    sleep "$TICK_WAIT_SECS"
    verify_caddy_removed
    verify_journal_empty
    echo "  remove + sweep: honest"

    stop_golemd "$PID"
done

echo
echo "M1 smoke test passed."
