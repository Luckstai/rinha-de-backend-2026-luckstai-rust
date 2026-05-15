#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INPUT="${1:-$ROOT/fixtures/official/resources/references.json.gz}"
OUTPUT="${2:-$ROOT/fixtures/official/resources/references.idx}"
IMAGE="luckstai-rinha-api-dev"

abs_path() {
  local value="$1"
  local dir
  dir="$(cd "$(dirname "$value")" && pwd)"
  printf '%s/%s\n' "$dir" "$(basename "$value")"
}

INPUT="$(abs_path "$INPUT")"
OUTPUT="$(abs_path "$OUTPUT")"

docker build -t "$IMAGE" "$ROOT" >/dev/null

mkdir -p "$(dirname "$OUTPUT")"

docker run --rm \
  --entrypoint /app/build-index \
  -v "$INPUT":"$INPUT":ro \
  -v "$(dirname "$OUTPUT")":"$(dirname "$OUTPUT")" \
  "$IMAGE" \
  "$INPUT" "$OUTPUT"
