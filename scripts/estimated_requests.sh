#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - <<'PY' "$ROOT/test/test.js"
import re
import sys
from pathlib import Path

test_file = Path(sys.argv[1]).read_text()

start_match = re.search(r"startRate:\s*(\d+)", test_file)
if not start_match:
    raise SystemExit("could not find startRate in test/test.js")

start_rate = int(start_match.group(1))
stages = re.findall(r"\{\s*duration:\s*'(\d+)s',\s*target:\s*(\d+)\s*\}", test_file)

headers = ("stage", "duration", "from", "to", "stage_reqs", "cumulative")
rows = []
prev_rate = start_rate
cumulative = 0
total_duration = 0

for index, (duration_s, target) in enumerate(stages, start=1):
    duration = int(duration_s)
    target = int(target)
    stage_reqs = (prev_rate + target) * duration // 2
    cumulative += stage_reqs
    total_duration += duration
    rows.append((str(index), f"{duration}s", str(prev_rate), str(target), str(stage_reqs), str(cumulative)))
    prev_rate = target

rows.append(("total", f"{total_duration}s", str(start_rate), str(prev_rate), "", str(cumulative)))

widths = [len(header) for header in headers]
for row in rows:
    for index, column in enumerate(row):
        widths[index] = max(widths[index], len(column))

def hline(left, mid, right):
    print(left + mid.join("─" * (width + 2) for width in widths) + right)

def print_row(columns):
    print("│" + "│".join(f" {column:>{widths[index]}} " for index, column in enumerate(columns)) + "│")

hline("┌", "┬", "┐")
print_row(headers)
hline("├", "┼", "┤")
for row in rows[:-1]:
    print_row(row)
hline("├", "┼", "┤")
print_row(rows[-1])
hline("└", "┴", "┘")
PY
