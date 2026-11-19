#!/bin/bash
# The front-window read against the installed Attn Lens: the automation socket's
# screen_text with a TextEdit window of synthetic text over a Terminal window of other
# text, then with Terminal on top, then with Screen Recording taken away, where the
# read must fail without showing the permission guide. Writes TCC's database, so run it
# through `scripts/lens-vm.sh screen-text OUT`; never on a real Mac. Posts no input.
#   scripts/lens-screen-text.sh <output dir>
# Writes front.json, swapped.json, denied.json, windows-*.txt, lens.log and *.png.
# Exits non-zero on the first thing that does not hold.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:?usage: lens-screen-text.sh <output dir>}"
APP="$HOME/Applications/Attn Lens.app"
LENS="$APP/Contents/MacOS/AttnLens"
TCC="/Library/Application Support/com.apple.TCC/TCC.db"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

# Every on-screen window with its owner, front to back.
cat >"$OUT/windows.swift" <<'SWIFT'
import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
for w in list {
    print(w[kCGWindowOwnerName as String] ?? "?", w[kCGWindowLayer as String] ?? "?", w[kCGWindowName as String] ?? "", separator: "\t")
}
SWIFT
/usr/bin/swiftc -O "$OUT/windows.swift" -o "$OUT/windows"

quit_lens() {
    osascript -e 'if application id "dev.attn.lens" is running then tell application id "dev.attn.lens" to quit' \
        -e 'repeat while application id "dev.attn.lens" is running' -e 'delay 0.1' -e 'end repeat'
}
grant() {
    sudo sqlite3 "$TCC" "INSERT OR REPLACE INTO access (service,client,client_type,auth_value,auth_reason,auth_version,flags) VALUES ('kTCCServiceScreenCapture','dev.attn.lens',0,2,4,1,0);"
    sudo killall tccd 2>/dev/null || true
}
finish() {
    quit_lens || true
    grant
    osascript -e 'if application "TextEdit" is running then tell application "TextEdit" to quit saving no' \
        -e 'if application "Terminal" is running then tell application "Terminal" to quit' \
        -e 'if application "System Settings" is running then tell application "System Settings" to quit' || true
}
trap finish EXIT
launch() {
    open --stderr "$OUT/lens.log" "$APP" --args --automation
    until "$LENS" lens '{"cmd":"state"}' >/dev/null 2>&1; do sleep 0.1; done
}
read_screen() { "$LENS" lens '{"cmd":"screen_text"}' | tee "$OUT/$1.json"; echo; }
holds() { # file, word: the reply's text holds the word
    /usr/bin/python3 -I -c 'import json,sys; sys.exit(0 if sys.argv[2] in json.load(open(sys.argv[1])).get("text","") else 1)' "$1" "$2"
}
fail() { echo "FAIL $*"; exit 1; }

quit_lens
grant
# The guest agent drives Terminal through Apple Events, as guest-grant.sh lets it TextEdit.
sqlite3 "$HOME/Library/Application Support/com.apple.TCC/TCC.db" "INSERT OR REPLACE INTO access (service,client,client_type,auth_value,auth_reason,auth_version,indirect_object_identifier_type,indirect_object_identifier,flags) VALUES ('kTCCServiceAppleEvents','$(realpath /opt/homebrew/bin/tart-guest-agent)',1,2,4,1,0,'com.apple.Terminal',0);"
killall tccd 2>/dev/null || true
printf 'Behind window words: zebraOrchard TERMINAL_BEHIND quartz-lantern\n' >"$OUT/behind.txt"
printf 'Front window words: idleCompaction src/lens/dictation.rs sigma-env TEXTEDIT_FRONT\n' >"$OUT/front.txt"

# Terminal first, then TextEdit over it at the same bounds.
osascript -e 'tell application "Terminal"' -e 'activate' \
    -e "do script \"clear; cat '$OUT/behind.txt'\"" \
    -e 'repeat until (count of windows) > 0' -e 'delay 0.1' -e 'end repeat' \
    -e 'set bounds of window 1 to {100, 100, 900, 500}' -e 'end tell'
open -a TextEdit "$OUT/front.txt"
osascript -e 'tell application "TextEdit"' -e 'activate' \
    -e 'repeat until (count of windows) > 0' -e 'delay 0.1' -e 'end repeat' \
    -e 'set bounds of window 1 to {100, 100, 900, 500}' -e 'end tell'
sleep 1
launch
"$OUT/windows" >"$OUT/windows-front.txt"
screencapture -x "$OUT/front.png"
read_screen front
holds "$OUT/front.json" TEXTEDIT_FRONT || fail "the read missed TextEdit's text"
holds "$OUT/front.json" TERMINAL_BEHIND && fail "the read holds the covered Terminal's text"

# Terminal on top: the read follows z-order.
osascript -e 'tell application "Terminal" to activate'
sleep 1
"$OUT/windows" >"$OUT/windows-swapped.txt"
screencapture -x "$OUT/swapped.png"
read_screen swapped
holds "$OUT/swapped.json" TERMINAL_BEHIND || fail "the read missed Terminal's text"
holds "$OUT/swapped.json" TEXTEDIT_FRONT && fail "the read holds the covered TextEdit's text"
osascript -e 'tell application "TextEdit" to activate'

# Without Screen Recording. Launching shows the guide, as it always has; once System
# Settings closes, the guide closes, and the read must not bring either back.
quit_lens
sudo sqlite3 "$TCC" "DELETE FROM access WHERE service='kTCCServiceScreenCapture' AND client='dev.attn.lens';"
sudo killall tccd 2>/dev/null || true
launch
sleep 3
"$OUT/windows" >"$OUT/windows-launch-denied.txt"
osascript -e 'if application "System Settings" is running then tell application "System Settings" to quit'
for _ in $(seq 50); do
    "$OUT/windows" | grep -q '^Attn Lens' || break
    sleep 0.1
done
osascript -e 'tell application "TextEdit" to activate'
sleep 1
"$OUT/windows" >"$OUT/windows-before-denied.txt"
read_screen denied
/usr/bin/python3 -I -c 'import json,sys; r=json.load(open(sys.argv[1])); sys.exit(0 if r["ok"] is False and "Screen Recording" in r["error"] else 1)' \
    "$OUT/denied.json" || fail "the read without Screen Recording did not fail as expected"
sleep 3
"$OUT/windows" >"$OUT/windows-after-denied.txt"
screencapture -x "$OUT/denied.png" || true
grep '^Attn Lens' "$OUT/windows-after-denied.txt" && fail "a Lens window appeared after the denied read"
grep '^System Settings' "$OUT/windows-after-denied.txt" && fail "System Settings opened after the denied read"
cmp -s "$OUT/windows-before-denied.txt" "$OUT/windows-after-denied.txt" || fail "the windows changed after the denied read"
echo "PASS screen_text reads the front window and fails quietly without Screen Recording"
