#!/usr/bin/env bash
# run.sh — end-to-end demo: build the golemd image, run it as host "manta",
# commission the lichess-subset blueprints against it, and show that golem
# recorded and resolved them. Idempotent: nukes any prior container + state.
#
#   ./examples/lichess/run.sh
#
# Requires: docker, and `nickel` reachable (directly on PATH, or via
# `nix-shell -p nickel`, which this script falls back to automatically).
set -euo pipefail

IMAGE="golemd:lichess"
CONTAINER="golemd-lichess"
HOST_ID="manta"          # this golem's identity; only its actions hit the builder
PORT=7474
ADDR="http://127.0.0.1:${PORT}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BLUEPRINTS=(kaiju manta scaly orbit talos zulip)

# nickel shim: use nickel if present, else borrow it from nixpkgs.
if command -v nickel >/dev/null 2>&1; then
  nickel_export() { nickel export --format json "$1"; }
else
  echo "nickel not on PATH; using 'nix-shell -p nickel'." >&2
  nickel_export() { nix-shell -p nickel --run "nickel export --format json '$1'"; }
fi

curl_json() { curl -fsS "$@"; }

echo "=============================================================="
echo " 1. Build the golemd image"
echo "=============================================================="
docker build -t "$IMAGE" "$ROOT"

echo
echo "=============================================================="
echo " 2. (Re)start the container as host '$HOST_ID' on :$PORT"
echo "=============================================================="
docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
docker run -d --name "$CONTAINER" -p "${PORT}:7474" \
  -e GOLEM_HOST="$HOST_ID" "$IMAGE" >/dev/null
echo "container: $(docker ps --filter name=$CONTAINER --format '{{.Names}} {{.Status}}')"

echo
echo "Waiting for /status to come up..."
for i in $(seq 1 30); do
  if curl_json "$ADDR/status" >/dev/null 2>&1; then
    echo "healthy after ${i}s: $(curl_json "$ADDR/status")"
    break
  fi
  sleep 1
  if [ "$i" -eq 30 ]; then echo "golemd never became healthy" >&2; docker logs "$CONTAINER"; exit 1; fi
done

# commission one blueprint, retrying past RandomBuilder's simulated flakiness.
commission() {
  local name="$1" json status body
  json="$(nickel_export "$HERE/${name}.ncl")"
  for attempt in 1 2 3 4 5 6; do
    body="$(curl -sS -o /tmp/golem_resp.json -w '%{http_code}' \
      -H 'content-type: application/json' -X POST --data "$json" "$ADDR/blueprints")" || true
    status="$body"
    if [ "$status" = "200" ]; then
      echo "  commissioned $name (revision $(python3 -c 'import json,sys;print(json.load(open("/tmp/golem_resp.json"))["id"])'))"
      return 0
    fi
    echo "  $name attempt $attempt -> HTTP $status (RandomBuilder flaked); retrying" >&2
    sleep 1
  done
  echo "  FAILED to commission $name after retries:" >&2
  cat /tmp/golem_resp.json >&2; return 1
}

# decommission a blueprint, retrying past RandomBuilder's simulated flakiness on
# the teardown actions for this --host. On builder failure golemd rolls the
# whole decommission back (persists nothing) and returns 500 — so we just retry.
decommission() {
  local name="$1" status
  for attempt in 1 2 3 4 5 6 7 8; do
    status="$(curl -sS -o /tmp/golem_resp.json -w '%{http_code}' -X DELETE "$ADDR/blueprints/$name")" || true
    if [ "$status" = "200" ]; then cat /tmp/golem_resp.json; return 0; fi
    echo "  decommission $name attempt $attempt -> HTTP $status (RandomBuilder flaked); retrying" >&2
    sleep 1
  done
  echo "  FAILED to decommission $name after retries:" >&2; cat /tmp/golem_resp.json >&2; return 1
}

echo
echo "=============================================================="
echo " 3. Commission the lichess blueprints (+ a canary that shares manta/lila)"
echo "=============================================================="
for bp in "${BLUEPRINTS[@]}"; do commission "$bp"; done
# Second blueprint that also wants lila on manta -> exercises refcounting.
commission "manta-canary"

echo
echo "=============================================================="
echo " 4. Resolved STATE (per host: item -> owning blueprints)"
echo "=============================================================="
curl_json "$ADDR/state" | python3 -m json.tool

echo
echo "=============================================================="
echo " 5. STATUS  (host identity + latest revision id)"
echo "=============================================================="
curl_json "$ADDR/status" | python3 -m json.tool

echo
echo "=============================================================="
echo " 6. REVISION journal (id / kind / blueprint / #actions)"
echo "=============================================================="
curl_json "$ADDR/revisions" | python3 -c '
import json, sys
for r in json.load(sys.stdin):
    rid = r["id"]; kind = r["kind"]; bp = str(r["blueprint"]); n = len(r["actions"])
    print("  rev %2d  %-12s %-13s actions=%d" % (rid, kind, bp, n))
'

MANTA_REV="$(curl_json "$ADDR/revisions" | python3 -c '
import json,sys
revs=[r for r in json.load(sys.stdin) if r["blueprint"]=="manta" and r["kind"]=="commission"]
print(revs[-1]["id"])
')"
curl_json "$ADDR/revisions/$MANTA_REV" | python3 -m json.tool

echo
echo "=============================================================="
echo " 7. DECOMMISSION 'manta' — show the Teardown actions"
echo "=============================================================="
decommission manta | python3 -c '
import json, sys
r = json.load(sys.stdin)
print("  revision %s  kind=%s  blueprint=%s" % (r["id"], r["kind"], r["blueprint"]))
print("  actions:")
for a in r["actions"]:
    print("    %-16s %s/%s" % (a["step"], a["host"], a["name"]))
print("  manta still in state? ", "manta" in r["state"]["hosts"])
'


echo
echo "------ refcount proof: lila is wanted by BOTH 'manta' and 'manta-canary'."
echo "       Decommissioning 'manta' tears down manta-UNIQUE items but keeps"
echo "       lila standing (manta-canary still wants it). State of host manta: ------"
curl_json "$ADDR/state" | python3 -c '
import json,sys
s=json.load(sys.stdin)
m=s["hosts"].get("manta",{})
def show(label,d):
    for k in sorted(d): print(f"    {label:<9} {k:<22} wanted_by={sorted(d[k])}")
show("workload", m.get("workloads",{}))
show("service",  m.get("services",{}))
show("ingress",  m.get("ingress",{}))
assert "lila" in m.get("services",{}), "lila should survive"
print("  -> lila SURVIVED (refcount held by manta-canary). PASS")
'

echo
echo "Done. Container '$CONTAINER' left running (docker rm -f $CONTAINER to stop)."
