#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_REPO="${1:-${RINHA_OFFICIAL_REPO_PATH:-}}"
DEST="$ROOT/fixtures/official"

if [[ -z "$SOURCE_REPO" ]]; then
  printf 'missing official repo path\n' >&2
  printf 'use: RINHA_OFFICIAL_REPO_PATH=/path/to/rinha-de-backend-2026 ./scripts/sync_official_fixtures.sh\n' >&2
  printf 'or:  ./scripts/sync_official_fixtures.sh /path/to/rinha-de-backend-2026\n' >&2
  exit 1
fi

if [[ ! -d "$SOURCE_REPO/resources" || ! -d "$SOURCE_REPO/test" ]]; then
  printf 'invalid official repo path: %s\n' "$SOURCE_REPO" >&2
  exit 1
fi

mkdir -p "$DEST/resources" "$DEST/test"
mkdir -p "$ROOT/test"

cp "$SOURCE_REPO/resources/normalization.json" "$DEST/resources/normalization.json"
cp "$SOURCE_REPO/resources/mcc_risk.json" "$DEST/resources/mcc_risk.json"
cp "$SOURCE_REPO/resources/example-references.json" "$DEST/resources/example-references.json"
cp "$SOURCE_REPO/resources/references.json.gz" "$DEST/resources/references.json.gz"
cp "$SOURCE_REPO/test/test-data.json" "$DEST/test/test-data.json"
cp "$SOURCE_REPO/test/test-data.json" "$ROOT/test/test-data.json"
cp "$SOURCE_REPO/test/test.js" "$ROOT/test/test.js"
cp "$SOURCE_REPO/test/smoke.js" "$ROOT/test/smoke.js"

printf 'synced official fixtures into %s\n' "$DEST"
