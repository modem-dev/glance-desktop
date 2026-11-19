#!/usr/bin/env python3
"""Prints p50, p99 and longest of a list of milliseconds read from JSON lines.
FIELD may name a nested field with dots, such as quality.cpu_ms.
  summary.py FILE FIELD [--skip-first N] [--name NAME]"""
import json, sys

path, field = sys.argv[1], sys.argv[2]
skip = int(sys.argv[sys.argv.index("--skip-first") + 1]) if "--skip-first" in sys.argv else 0
values = []
for line in open(path):
    row = json.loads(line)
    if "end" in row:
        continue
    for key in field.split("."):
        row = row.get(key) if isinstance(row, dict) else None
    if row is not None:
        values.append(row)
values = sorted(values[skip:])
if not values:
    sys.exit(f"{path}: no {field}")
pick = lambda q: values[min(len(values) - 1, int(q * len(values)))]
print(f"{field}: n={len(values)} p50={pick(.5):.2f} p99={pick(.99):.2f} longest={values[-1]:.2f} ms")
