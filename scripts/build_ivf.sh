#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLAT_INDEX="${1:-$ROOT/fixtures/official/resources/references.idx}"
IVF_INDEX="${2:-$ROOT/fixtures/official/resources/references.ivf}"
NLIST="${3:-512}"
SAMPLE_SIZE="${4:-32768}"
ITERATIONS="${5:-8}"
PARTITION_BITS="${6:-0}"
IMAGE="luckstai-rinha-api-dev"

abs_path() {
  local value="$1"
  local dir
  dir="$(cd "$(dirname "$value")" && pwd)"
  printf '%s/%s\n' "$dir" "$(basename "$value")"
}

FLAT_INDEX="$(abs_path "$FLAT_INDEX")"
IVF_INDEX="$(abs_path "$IVF_INDEX")"

docker build -t "$IMAGE" "$ROOT" >/dev/null
mkdir -p "$(dirname "$IVF_INDEX")"

docker run --rm \
  --entrypoint /app/build-ivf \
  -v "$FLAT_INDEX":"$FLAT_INDEX":ro \
  -v "$(dirname "$IVF_INDEX")":"$(dirname "$IVF_INDEX")" \
  "$IMAGE" \
  "$FLAT_INDEX" "$IVF_INDEX" "$NLIST" "$SAMPLE_SIZE" "$ITERATIONS" "$PARTITION_BITS"
