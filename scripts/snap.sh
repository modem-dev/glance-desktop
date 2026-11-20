#!/usr/bin/env bash
# Renders every scene of Attn Lens's windows headless, in dark and light, and lays
# them out on two contact sheets. It keeps no list of its own: the windows come from
# `--list-windows` and each window's scenes from `--list-scenes`.
#
# usage: scripts/snap.sh OUT [WINDOW...]
#   OUT/WINDOW--SCENE--THEME.png  one render per scene and theme
#   OUT/dark.png, OUT/light.png   the contact sheets (ImageMagick's montage)
# SIZE=900x560 renders the editor at that size, such as its smallest. Settings renders
# at its own size. The windows Lens places over the screen render on a 1440x900 desktop.
# Prints the two sheets; a scene that fails to render prints its error and the
# script exits 1 after rendering the rest.
set -uo pipefail
[ $# -ge 1 ] || { echo "usage: $0 OUT [WINDOW...]" >&2; exit 2; }
repo=$(cd "$(dirname "$0")/.." && pwd)
out=$1
shift
command -v montage >/dev/null || { echo "snap.sh needs montage, from ImageMagick" >&2; exit 2; }
cargo build --quiet --locked --manifest-path "$repo/Cargo.toml" || exit 2
bin=${CARGO_TARGET_DIR:-$repo/target}/debug/attn-lens
# The capture scenes list sessions through attn: never this machine's.
export ATTN_BIN=$repo/tools/scenes/attn
mkdir -p "$out"
err=$(mktemp)
# Each render saves settings into a directory of its own, never this machine's
# prefs, so a scene that changes a setting does not leak into the next render.
prefs=$(mktemp -d)
trap 'rm -rf "$err" "$prefs"' EXIT

if [ $# -gt 0 ]; then windows=("$@"); else mapfile -t windows < <("$bin" --list-windows); fi
failed=0
dark=() light=()
for window in "${windows[@]}"; do
    scenes=$("$bin" --window "$window" --list-scenes) || { failed=1; continue; }
    for scene in $scenes; do
        for theme in dark light; do
            png=$out/$window--$scene--$theme.png
            # Lens places every window but the editor and Settings over the screen,
            # at a size it sets, so those render on a desktop with the rest of the capture.
            if [ "$window" = editor ]; then
                where=(${SIZE:+--size "$SIZE"})
            elif [ "$window" = settings ]; then
                where=()
            else
                where=(--desktop "$repo/headless/editor/fixture.png" --size 1440x900)
            fi
            rm -rf "${prefs:?}"/*
            if RUNE_PREFS_DIR=$prefs "$bin" --window "$window" --scene "$scene" --theme "$theme" --scale 1 \
                "${where[@]}" --at 1 --snap "$png" >/dev/null 2>"$err"; then
                if [ "$theme" = dark ]; then dark+=("$png"); else light+=("$png"); fi
            else
                echo "FAIL $window $scene $theme:" >&2
                tail -5 "$err" >&2
                failed=1
            fi
        done
    done
done

sheet() { # theme files...
    local theme=$1
    shift
    [ $# -gt 0 ] || return 0
    local font
    font=$(fc-match -f '%{file}' sans 2>/dev/null) || font=
    montage -label '%t' ${font:+-font "$font"} -pointsize 14 -geometry '640x640>+12+12' \
        -tile 4x -background '#888888' "$@" "$out/$theme.png" && echo "$out/$theme.png"
}
sheet dark "${dark[@]}"
sheet light "${light[@]}"
exit $failed
