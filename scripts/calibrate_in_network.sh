#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_RPS="${TARGET_RPS:-200}"
DURATION="${DURATION:-30s}"
TARGET_BASE_URL="${TARGET_BASE_URL:-http://lb:9999}"
NETWORK_NAME="${NETWORK_NAME:-}"
K6_IMAGE="${K6_IMAGE:-grafana/k6:1.1.0}"

export K6_NO_USAGE_REPORT=true

"$ROOT/scripts/prepare_local_test.sh"

if [[ -z "$NETWORK_NAME" ]]; then
  NETWORK_NAME="$(
    docker inspect rinha-lb \
      --format '{{range $name, $_ := .NetworkSettings.Networks}}{{println $name}}{{end}}' \
      | head -n 1 \
      | tr -d '[:space:]'
  )"
fi

if [[ -z "$NETWORK_NAME" ]]; then
  printf 'failed to resolve compose network for rinha-lb\n' >&2
  exit 1
fi

docker run --rm \
  --network "$NETWORK_NAME" \
  -e K6_NO_USAGE_REPORT=true \
  -e TARGET_RPS="$TARGET_RPS" \
  -e DURATION="$DURATION" \
  -e TARGET_BASE_URL="$TARGET_BASE_URL" \
  -v "$ROOT":/workspace:rw \
  -w /workspace \
  "$K6_IMAGE" \
  run /workspace/test/calibrate.js >/dev/null

cat "$ROOT/test/results.calibrate.json" | jq
