#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

ensure_file() {
  local path="$1"
  local hint="$2"
  if [[ ! -f "$path" ]]; then
    printf 'missing required file: %s\n%s\n' "$path" "$hint" >&2
    exit 1
  fi
}

if [[ ! -f "$ROOT/test/test.js" || ! -f "$ROOT/test/test-data.json" ]]; then
  "$ROOT/scripts/sync_official_fixtures.sh"
fi

ensure_file "$ROOT/fixtures/official/resources/normalization.json" \
  "run ./scripts/sync_official_fixtures.sh first"
ensure_file "$ROOT/fixtures/official/resources/mcc_risk.json" \
  "run ./scripts/sync_official_fixtures.sh first"
ensure_file "$ROOT/fixtures/official/resources/references.json.gz" \
  "run ./scripts/sync_official_fixtures.sh first"

if [[ ! -f "$ROOT/fixtures/official/resources/references.idx" ]]; then
  printf 'references.idx not found, building quantized index...\n'
  "$ROOT/scripts/build_index.sh"
fi

if [[ ! -f "$ROOT/fixtures/official/resources/references.n2048.s65536.i8.ivf" ]]; then
  printf 'best ivf variant not found, building references.n2048.s65536.i8.ivf...\n'
  "$ROOT/scripts/build_ivf.sh" \
    "$ROOT/fixtures/official/resources/references.idx" \
    "$ROOT/fixtures/official/resources/references.n2048.s65536.i8.ivf" \
    2048 \
    65536 \
    8
fi

docker compose -f "$ROOT/docker-compose.yml" up -d --build

for _ in $(seq 1 30); do
  if curl -fsS "http://localhost:9999/ready" >/dev/null 2>&1; then
    printf 'local stack is ready on http://localhost:9999\n'
    exit 0
  fi
  sleep 1
done

printf 'local stack did not become ready in time\n' >&2
docker compose -f "$ROOT/docker-compose.yml" logs --tail=50
exit 1
