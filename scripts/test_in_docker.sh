#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

docker run --rm \
  --entrypoint /bin/bash \
  -v "$ROOT":/app \
  -w /app \
  rust:1.89-bookworm \
  -lc "export PATH=/usr/local/cargo/bin:\$PATH && cargo test"
