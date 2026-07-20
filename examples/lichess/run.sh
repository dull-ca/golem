#!/usr/bin/env bash
#
# End-to-end demo of the lichess fleet: compile the Emet program to a binary
# manifest, run golemd in a container as one host, apply the manifest, and show
# the resolved state and journal. golemd selects and enacts only the scroll for
# HOST_ID (`manta` below); the same manifest drives every host.
#
# The pipeline is emetc -> manifest -> golemctl apply; there is no Nickel step.
set -euo pipefail

IMAGE="golemd:lichess"
CONTAINER="golemd-lichess"
HOST_ID="manta"          # which host's scroll this golemd instance enacts
PORT=7474
ADDR="http://127.0.0.1:${PORT}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
FLEET="$HERE/fleet.emet"       # the Emet entry module (the whole fleet)
MANIFEST="$HERE/fleet.manifest" # emetc's binary, content-addressed output

curl_json() { curl -fsS "$@"; }

echo "=============================================================="
echo " 1. Compile the fleet to a binary manifest"
echo "=============================================================="
( cd "$ROOT" && cargo run -q -p emet -- build "$FLEET" -o "$MANIFEST" )
echo "wrote $MANIFEST ($(wc -c < "$MANIFEST") bytes)"
( cd "$ROOT" && cargo run -q -p emet -- build "$FLEET" --text )

echo
echo "=============================================================="
echo " 2. Build the golemd image"
echo "=============================================================="
docker build -t "$IMAGE" "$ROOT"

echo
echo "=============================================================="
echo " 3. (Re)start the container as host '$HOST_ID' on :$PORT"
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

echo
echo "=============================================================="
echo " 4. Apply the manifest — golemd selects host '$HOST_ID's scroll"
echo "=============================================================="
( cd "$ROOT" && cargo run -q -p golemctl -- apply "$MANIFEST" "$ADDR" )

echo
echo "=============================================================="
echo " 5. Resolved STATE for host '$HOST_ID'"
echo "=============================================================="
curl_json "$ADDR/state" | python3 -m json.tool

echo
echo "=============================================================="
echo " 6. REVISION journal"
echo "=============================================================="
curl_json "$ADDR/revisions" | python3 -m json.tool

echo
echo "Done. Container '$CONTAINER' left running (docker rm -f $CONTAINER to stop)."
