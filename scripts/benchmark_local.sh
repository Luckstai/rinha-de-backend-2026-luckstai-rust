#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export K6_NO_USAGE_REPORT=true

"$ROOT/scripts/prepare_local_test.sh"

k6 run "$ROOT/test/test.js" >/dev/null

cat "$ROOT/test/results.json" | jq
