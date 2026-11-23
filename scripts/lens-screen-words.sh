#!/bin/bash
# Screen words against the installed Attn Lens, with tools/vm/attn as attn: TextEdit
# in front shows ScopeTestEnvironment and "skill" twice, and a dictation of a synthetic
# voice saying "rename scope test environment and load the scale" sends the
# identifier in screen_words and scale/skill in maybe_misheard. Then a dictation that
# starts over that window and moves to a second, which shows fetch_release_notes,
# sends the identifiers of both, and pastes in the second. Posts real keys and
# writes the Keychain, so run it through `scripts/lens-vm.sh screen-words OUT`; never
# on a real Mac.
#   scripts/lens-screen-words.sh <output dir>
# Writes driver.log, lens.log (LENS_DICTATION_LOG=1), model-request.json (the whole
# request the stand-in got), input.json, textedit.txt and screen-words.mov, and for
# the second dictation switch-driver.log, switch-model-request.json, switch-input.json,
# switch-textedit.txt and screen-words-switch.mov. Exits
# non-zero on the first thing that does not hold.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:?usage: lens-screen-words.sh <output dir>}"
APP="$HOME/Applications/Attn Lens.app"
LENS="$APP/Contents/MacOS/AttnLens"
SENT="$HOME/lens-check/sent"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

/usr/bin/swiftc -O tools/driver/driver.swift -o "$OUT/driver"

# The speech, synthetic: the system voice, as 16 kHz mono like the fixtures.
say -o "$OUT/spoken.aiff" "rename scope test environment and load the scale"
afconvert -f WAVE -d LEI16@16000 -c 1 "$OUT/spoken.aiff" "$OUT/spoken.wav"
say -o "$OUT/switch.aiff" "rename scope test environment and call fetch release notes"
afconvert -f WAVE -d LEI16@16000 -c 1 "$OUT/switch.aiff" "$OUT/switch.wav"

RECORDER=
quit_lens() {
    osascript -e 'if application id "dev.attn.lens" is running then tell application id "dev.attn.lens" to quit' \
        -e 'repeat while application id "dev.attn.lens" is running' -e 'delay 0.1' -e 'end repeat'
}
finish() {
    [ -z "$RECORDER" ] || kill -KILL "$RECORDER" 2>/dev/null || true
    quit_lens || true
    osascript -e 'if application "TextEdit" is running then tell application "TextEdit" to quit saving no' || true
}
trap finish EXIT

# Pairs with tools/vm/attn as lens-dictation.sh does, and starts with the setting on.
quit_lens
security delete-generic-password -s dev.attn.lens.sender -a default >/dev/null 2>&1 || true
rm -f "$HOME/Library/Application Support/dev.attn.lens/prefs.json"
rm -f "$SENT/model-request" "$SENT/model-args"
open --env LENS_DICTATION_LOG=1 --stderr "$OUT/lens.log" "$APP" --args --automation
until "$LENS" lens '{"cmd":"state"}' >/dev/null 2>&1; do sleep 0.1; done
attn_status() { "$LENS" lens '{"cmd":"state"}' | /usr/bin/python3 -I -c 'import json,sys; print(json.load(sys.stdin)["state"]["attn"])'; }
"$LENS" lens '{"cmd":"key","key":"capture"}' >/dev/null
"$LENS" lens '{"cmd":"pick"}' >/dev/null
"$LENS" lens '{"cmd":"key","key":"send"}' >/dev/null
for _ in $(seq 100); do [ "$(attn_status)" = Ready ] && break; sleep 0.1; done
[ "$(attn_status)" = Ready ] || { echo "FAIL Lens did not pair: $(attn_status)"; exit 1; }
"$LENS" lens '{"cmd":"end"}' >/dev/null
"$LENS" lens '{"cmd":"discard"}' >/dev/null
"$LENS" lens "{\"cmd\":\"voice-wav\",\"path\":\"$OUT/spoken.wav\"}" >/dev/null
# A first dictation fetches and loads the speech models, which a new VM lacks; the
# driven one then hears from its first word. Up to 15 minutes for the 731 MB download.
"$LENS" lens '{"cmd":"key","key":"voice"}' >/dev/null
for _ in $(seq 1800); do "$LENS" lens '{"cmd":"state"}' | grep -qi listening && break; sleep 0.5; done
"$LENS" lens '{"cmd":"state"}' | grep -qi listening || { echo "FAIL voice never listened"; exit 1; }
"$LENS" lens '{"cmd":"key","key":"voice"}' >/dev/null
until "$LENS" lens '{"cmd":"state"}' | grep -q '"dictation":null'; do sleep 0.2; done
rm -f "$SENT/model-request" "$SENT/model-args"

printf 'Rename ScopeTestEnvironment before Friday.\nRead the skill first; the skill lists the rules.\n\n' >"$OUT/note.txt"
open -a TextEdit "$OUT/note.txt"
osascript -e 'tell application "TextEdit"' -e 'activate' \
    -e 'repeat until (count of windows) > 0' -e 'delay 0.1' -e 'end repeat' \
    -e 'set bounds of window 1 to {100, 100, 900, 500}' \
    -e 'set size of text of document 1 to 24' -e 'end tell'

screencapture -v -V 30 "$OUT/screen-words.mov" &
RECORDER=$!

cat >"$OUT/script.txt" <<'SCRIPT'
mark start
click 500 400
wait 500
tap ropt
wait 6000
tap ropt
expect "note":"Pasted"
lens {"cmd":"state"}
mark pasted
SCRIPT

"$OUT/driver" --front TextEdit "$OUT/script.txt" | tee "$OUT/driver.log"
osascript -e 'tell application "TextEdit" to get text of document 1' >"$OUT/textedit.txt"
cp "$SENT/model-request" "$OUT/model-request.json"
wait "$RECORDER" || true
RECORDER=

/usr/bin/python3 -I - "$OUT" <<'PY'
import json, pathlib, re, sys
out = pathlib.Path(sys.argv[1])
def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what)
    if not ok:
        sys.exit(1)
request = json.loads((out / "model-request.json").read_text())
model_input = json.loads(request["input"])
(out / "input.json").write_text(json.dumps(model_input, indent=1))
check("ScopeTestEnvironment" in model_input.get("screen_words", []),
      "ScopeTestEnvironment went in screen_words: %s" % model_input)
check({"heard": "scale", "screen": "skill"} in model_input.get("maybe_misheard", []),
      "scale/skill went in maybe_misheard: %s" % model_input.get("maybe_misheard"))
log = (out / "lens.log").read_text()
read = re.search(r"dictation \d+: screen read 0 in (\d+) ms, (\d+) words", log)
check(read is not None, "the log has the read's time and word count: %s" % (read and read.group(0)))
counts = re.search(r"dictation \d+: (\d+) of (\d+) screen words match, (\d+) sound pairs", log)
check(counts is not None, "the log has the matched and pair counts: %s" % (counts and counts.group(0)))
check("ScopeTestEnvironment" not in log and "skill" not in log, "the log holds no screen words")
PY

# The second dictation starts over note.txt and moves to note2.txt, a second TextEdit
# window below it: a switch inside one app, which only the poll of the front window sees.
"$LENS" lens "{\"cmd\":\"voice-wav\",\"path\":\"$OUT/switch.wav\"}" >/dev/null
rm -f "$SENT/model-request" "$SENT/model-args"
printf 'Then call fetch_release_notes from the release script.\n\n' >"$OUT/note2.txt"
open -a TextEdit "$OUT/note2.txt"
osascript -e 'tell application "TextEdit"' -e 'activate' \
    -e 'repeat until (count of windows) > 1' -e 'delay 0.1' -e 'end repeat' \
    -e 'set bounds of window 1 to {100, 400, 900, 700}' \
    -e 'set size of text of document 1 to 24' \
    -e 'set bounds of window 2 to {100, 60, 900, 360}' -e 'end tell'

screencapture -v -V 30 "$OUT/screen-words-switch.mov" &
RECORDER=$!

cat >"$OUT/switch-script.txt" <<'SCRIPT'
mark start
click 500 250
wait 800
tap ropt
wait 2500
mark switch
click 500 600
wait 5000
tap ropt
expect "note":"Pasted"
lens {"cmd":"state"}
mark pasted
SCRIPT

"$OUT/driver" --front TextEdit "$OUT/switch-script.txt" | tee "$OUT/switch-driver.log"
osascript -e 'tell application "TextEdit" to get text of document "note2.txt"' >"$OUT/switch-textedit.txt"
cp "$SENT/model-request" "$OUT/switch-model-request.json"
wait "$RECORDER" || true
RECORDER=

/usr/bin/python3 -I - "$OUT" <<'PY'
import json, pathlib, re, sys
out = pathlib.Path(sys.argv[1])
def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what)
    if not ok:
        sys.exit(1)
request = json.loads((out / "switch-model-request.json").read_text())
model_input = json.loads(request["input"])
(out / "switch-input.json").write_text(json.dumps(model_input, indent=1))
words = model_input.get("screen_words", [])
check("ScopeTestEnvironment" in words and "fetch_release_notes" in words,
      "both windows' identifiers went in screen_words: %s" % model_input)
log = (out / "lens.log").read_text()
last = log.split("voice key: dictation")[-1]
reads = re.findall(r"screen read (\d+) in (\d+) ms, (\d+) words", last)
check(len(reads) == 2, "the dictation read two windows: %s" % reads)
check(re.search(r"from 2 reads", last) is not None, "the refinement used both reads")
check("Refined: " in (out / "switch-textedit.txt").read_text(), "the text pasted in the second window")
check("fetch_release_notes" not in log, "the log holds no screen words")
PY
