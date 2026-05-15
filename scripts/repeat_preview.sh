#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNS="${RUNS:-3}"
PREFIX="${PREFIX:-results.repeat}"

export K6_NO_USAGE_REPORT=true

run_once() {
  local run_id="$1"
  local output="$ROOT/test/${PREFIX}.run${run_id}.json"

  "$ROOT/run.sh" >/dev/null
  cp "$ROOT/test/results.json" "$output"

  jq -r --arg run "$run_id" --arg file "$(basename "$output")" '
    [
      $run,
      .scoring.final_score,
      .p99,
      .scoring.breakdown.false_positive_detections,
      .scoring.breakdown.false_negative_detections,
      .scoring.breakdown.http_errors,
      $file
    ] | @tsv
  ' "$output"
}

{
  printf "run\tfinal_score\tp99\tfp\tfn\thttp_errors\tfile\n"
  for run_id in $(seq 1 "$RUNS"); do
    run_once "$run_id"
  done
} | tee "$ROOT/test/${PREFIX}.summary.tsv"
