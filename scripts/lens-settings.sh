#!/bin/bash
# The Settings window's Mac check, run in the VM by `scripts/lens-vm.sh settings OUT`:
# ⌘, with Lens in front opens it, as does the Dock menu's Settings…; a click flips
# "Use words on screen" and saves it; ⌘W closes the window; after a quit and a reopen
# the window shows the flipped value. The driver posts real input, so this runs only
# in the VM. Writes receipts.txt, driver.log and screenshots to OUT; any failed check
# stops it with the reason.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:?usage: lens-settings.sh OUT}"
APP="$HOME/Applications/Attn Lens.app"
LENS="$APP/Contents/MacOS/AttnLens"
PREFS="$HOME/Library/Application Support/dev.attn.lens/prefs.json"
mkdir -p "$OUT"
/usr/bin/swiftc -O tools/driver/driver.swift -o "$OUT/driver"
/usr/bin/swiftc -O tools/driver/ax.swift -o "$OUT/ax"
RECEIPTS="$OUT/receipts.txt"
: >"$RECEIPTS"

say() { echo "$*" | tee -a "$RECEIPTS"; }
fail() {
    say "FAIL: $*"
    exit 1
}
drive() { # one driver script per step, from stdin
    cat >"$OUT/step.txt"
    "$OUT/driver" --front "Attn Lens" "$OUT/step.txt" >>"$OUT/driver.log" ||
        fail "driver: $(tail -1 "$OUT/driver.log")"
}
field() { # IDENTIFIER KEY: one attribute of the element with that identifier
    "$OUT/ax" tree "Attn Lens" | /usr/bin/python3 -I -c '
import json, sys
for line in sys.stdin:
    row = json.loads(line)
    if row.get("identifier") == sys.argv[1]:
        value = row.get(sys.argv[2])
        print(" ".join(map(str, value)) if isinstance(value, list) else value)
        break' "$1" "$2"
}
# Accessibility starts when a client first asks, so the first reads can come back empty.
# 100 × 0.1 s is far past the one empty read the editor check saw.
wait_for() { # IDENTIFIER KEY EXPECTED
    for _ in $(seq 1 100); do
        [ "$(field "$1" "$2")" = "$3" ] && return
        sleep 0.1
    done
    fail "$1 $2 is '$(field "$1" "$2")', expected '$3'"
}
center() { field "$1" frame | awk '{ printf "%d %d", $1 + $3 / 2, $2 + $4 / 2 }'; }
state() { "$LENS" lens '{"cmd":"state"}'; }
# PYTHON-EXPRESSION over the automation state `s`, polled until true. 100 × 0.1 s is far
# past the few frames a window or a save takes.
wait_state() {
    for _ in $(seq 1 100); do
        state | /usr/bin/python3 -I -c 'import json,sys; s=json.load(sys.stdin)["state"]; sys.exit(0 if eval(sys.argv[1]) else 1)' "$1" && return
        sleep 0.1
    done
    fail "state never had $1: $(state)"
}
quit_lens() {
    osascript -e 'if application id "dev.attn.lens" is running then tell application id "dev.attn.lens" to quit' \
        -e 'repeat while application id "dev.attn.lens" is running' -e 'delay 0.1' -e 'end repeat'
}
launch() {
    open "$APP" --args --automation
    until state >/dev/null 2>&1; do sleep 0.1; done
    # Menus come once the event loop runs; 100 × 0.1 s is past any launch seen.
    for _ in $(seq 1 100); do
        "$OUT/ax" menus "Attn Lens" 2>/dev/null | grep -qxF "Attn Lens > Settings…" && return
        sleep 0.1
    done
    fail "no Settings… in the app menu: $("$OUT/ax" menus "Attn Lens" | head -20)"
}
# After the switch's motion settles, which a freshly shown window was caught in.
shot() {
    sleep 0.5
    screencapture -x "$OUT/$1.png"
}

quit_lens
rm -f "$PREFS"
launch
"$OUT/ax" menus "Attn Lens" | grep -F "Settings" | tee -a "$RECEIPTS"
wait_state 's["windows"]["settings"] is False and s["settings"]["screen_words"] is True'
say "launched: settings window hidden, screen_words on, no prefs.json"

say "--- ⌘, with Lens in front"
# The driver posts keys only while a Lens window is in front, so the editor comes first.
drive <<<'dock Attn Lens'
wait_state 's["windows"]["editor"] is True'
drive <<<'chord cmd comma 40'
wait_state 's["windows"]["settings"] is True'
wait_for screen-words-control value 1
say "⌘, opened Settings; switch value 1 ($(field screen-words-control role))"
shot opened

say "--- click the switch"
read -r x y <<<"$(center screen-words-control)"
drive <<<"click $x $y"
wait_state 's["settings"]["screen_words"] is False and s["settings_error"] is None'
wait_for screen-words-control value 0
say "click at $x $y: screen_words off, switch value 0; prefs.json: $(tr -d '\n ' <"$PREFS")"
grep -q '"dictation.screen_words": false' "$PREFS" || fail "prefs.json does not hold false"
shot flipped

say "--- ⌘W"
drive <<<'chord cmd w 40'
wait_state 's["windows"]["settings"] is False and s["windows"]["editor"] is True'
say "⌘W closed Settings; the editor behind it stays open"

say "--- quit and reopen"
quit_lens
launch
wait_state 's["settings"]["screen_words"] is False and s["windows"]["settings"] is False'
say "relaunched: screen_words off, window hidden"

say "--- the Dock menu's Settings…"
"$OUT/ax" dock-menu "Attn Lens" "Settings…"
wait_state 's["windows"]["settings"] is True'
wait_for screen-words-control value 0
say "Dock menu opened Settings; switch value 0"
shot reopened

# Leave the VM as it was found.
read -r x y <<<"$(center screen-words-control)"
drive <<<"click $x $y"
wait_state 's["settings"]["screen_words"] is True'
quit_lens
say "PASS"
