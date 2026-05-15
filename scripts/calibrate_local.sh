#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_RPS="${TARGET_RPS:-200}"
DURATION="${DURATION:-30s}"

export K6_NO_USAGE_REPORT=true

"$ROOT/scripts/prepare_local_test.sh"

TARGET_RPS="$TARGET_RPS" DURATION="$DURATION" \
  k6 run "$ROOT/test/calibrate.js" >/dev/null

cat "$ROOT/test/results.calibrate.json" | jq
