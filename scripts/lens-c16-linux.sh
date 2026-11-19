#!/bin/bash
# C16's Linux half: the scripted 60 s capture through Rune's headless runner and the
# fake port. Reports each frame's CPU time (Rune's --trace cpu_ms, from the frame's
# start to its GPU submit) only, because software Vulkan makes GPU time unrepresentative.
#   scripts/lens-c16-linux.sh <output dir>
# Writes script.json, frames.jsonl, resources.json, ink.json and summary.txt.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="$(mkdir -p "${1:?usage: lens-c16-linux.sh <output dir>}" && cd "$1" && pwd)"
cargo build --release --locked
python3 tools/c16/script.py >"$OUT/script.json"
# Any backdrop will do: a 1x1 PNG, base64-encoded.
base64 -d >"$OUT/backdrop.png" <<'PNG'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==
PNG
export ATTN_BIN=/bin/false
RUN=(target/release/attn-lens --desktop "$OUT/backdrop.png" --fps 60 --script "$OUT/script.json")
python3 tools/c16/run.py "$OUT/resources.json" \
    "${RUN[@]}" --duration 62 --trace "$OUT/frames.jsonl"
"${RUN[@]}" --at mark:end --window ink --tree "$OUT/ink.json"
{
    # The first 60 frames open the capture, before any drawing.
    python3 tools/c16/summary.py "$OUT/frames.jsonl" quality.cpu_ms --skip-first 60
    # Headless runs ahead of the clock, so CPU is seconds spent per 62 s of capture.
    cat "$OUT/resources.json"; echo
    echo "ink elements: $(grep -c '"key"' "$OUT/ink.json")"
} | tee "$OUT/summary.txt"
