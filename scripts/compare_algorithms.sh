#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIMIT="${1:-2000}"
IVF_NPROBE="${IVF_NPROBE:-4}"
IVF_INDEX_PATH="${IVF_INDEX_PATH:-$ROOT/fixtures/official/resources/references.ivf}"
IMAGE="luckstai-rinha-api-dev"

abs_path() {
  local value="$1"
  local dir
  dir="$(cd "$(dirname "$value")" && pwd)"
  printf '%s/%s\n' "$dir" "$(basename "$value")"
}

workspace_path() {
  local value="$1"
  value="$(abs_path "$value")"
  if [[ "$value" == "$ROOT/"* ]]; then
    printf '/workspace/%s\n' "${value#"$ROOT/"}"
    return 0
  fi
  printf 'ivf index path must be inside workspace: %s\n' "$value" >&2
  exit 1
}

IVF_INDEX_PATH="$(abs_path "$IVF_INDEX_PATH")"
IVF_INDEX_WORKSPACE_PATH="$(workspace_path "$IVF_INDEX_PATH")"

"$ROOT/scripts/prepare_local_test.sh" >/dev/null

if [[ ! -f "$IVF_INDEX_PATH" ]]; then
  "$ROOT/scripts/build_ivf.sh"
fi

docker build -t "$IMAGE" "$ROOT" >/dev/null

docker run --rm \
  --entrypoint /app/bench-algorithms \
  -e BENCH_DATASET=/workspace/test/test-data.json \
  -e BENCH_LIMIT="$LIMIT" \
  -e RINHA_NORMALIZATION_PATH=/workspace/fixtures/official/resources/normalization.json \
  -e RINHA_MCC_RISK_PATH=/workspace/fixtures/official/resources/mcc_risk.json \
  -e RINHA_INDEX_PATH=/workspace/fixtures/official/resources/references.idx \
  -e RINHA_IVF_INDEX_PATH="$IVF_INDEX_WORKSPACE_PATH" \
  -e RINHA_IVF_NPROBE="$IVF_NPROBE" \
  -v "$ROOT":/workspace:ro \
  "$IMAGE"
