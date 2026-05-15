#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIMIT="${LIMIT:-10000}"
IVF_NPROBE="${IVF_NPROBE:-4}"

variants=(
  "512 32768 8"
  "1024 65536 8"
  "1024 65536 12"
  "2048 65536 8"
)

for variant in "${variants[@]}"; do
  read -r nlist sample iterations <<<"$variant"
  ivf_path="$ROOT/fixtures/official/resources/references.n${nlist}.s${sample}.i${iterations}.ivf"

  if [[ ! -f "$ivf_path" ]]; then
    "$ROOT/scripts/build_ivf.sh" \
      "$ROOT/fixtures/official/resources/references.idx" \
      "$ivf_path" \
      "$nlist" \
      "$sample" \
      "$iterations" >/dev/null
  fi

  printf '=== nlist=%s sample=%s iterations=%s nprobe=%s ===\n' \
    "$nlist" "$sample" "$iterations" "$IVF_NPROBE"

  IVF_INDEX_PATH="$ivf_path" IVF_NPROBE="$IVF_NPROBE" \
    "$ROOT/scripts/compare_algorithms.sh" "$LIMIT"
  printf '\n'
done
