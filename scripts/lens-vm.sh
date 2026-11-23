#!/bin/bash
# A disposable macOS VM where the driver posts keys without touching this Mac's
# keyboard, pointer or screen. It has no shared folders and no credentials from
# this Mac; files go in through `tart exec` and the app is signed ad hoc.
#
#   scripts/lens-vm.sh setup             install Tart and make the VM
#   scripts/lens-vm.sh start | stop      boot it with no window on this Mac | stop it
#   scripts/lens-vm.sh status
#   scripts/lens-vm.sh push              build this checkout ad hoc and install it in the VM
#   scripts/lens-vm.sh driver SCRIPT [--front APP ...]
#                                        run a driver script in the VM's GUI session
#   scripts/lens-vm.sh check OUT         run lens-mac-check.sh in the VM against tools/vm/attn
#   scripts/lens-vm.sh live OUT          run lens-live.sh in the VM against tools/vm/attn
#   scripts/lens-vm.sh c16 OUT            run lens-c16.sh, C16's 60 s capture, in the VM
#   scripts/lens-vm.sh voice OUT [FIXTURE]
#                                        run lens-voice.sh in the VM with a speech fixture
#   scripts/lens-vm.sh editor OUT        run lens-editor-check.sh in the VM: menus, OCR, Pick
#                                        from screen, scroll, zoom, light and dark
#   scripts/lens-vm.sh tile OUT          run lens-tile.sh in the VM: tile hover and click, Back
#   scripts/lens-vm.sh switch OUT        run lens-switch.sh in the VM: draw, ⌘Tab, draw, ⌘Tab, draw
#   scripts/lens-vm.sh focus OUT         run lens-focus.sh in the VM: type, capture, end it, type again
#   scripts/lens-vm.sh keys OUT          run lens-keys.sh in the VM: right ⌘ and right ⌥ alone
#                                        and in chords with TextEdit, and the Accessibility guide
#   scripts/lens-vm.sh dictation OUT     run lens-dictation.sh in the VM: dictation into TextEdit,
#                                        focus and clipboard kept, and its timings
#   scripts/lens-vm.sh settings OUT      run lens-settings.sh in the VM: ⌘, and the Dock menu open
#                                        Settings, the switch saves, ⌘W closes, a relaunch keeps it
#   scripts/lens-vm.sh screen-text OUT   run lens-screen-text.sh in the VM: the front window's
#                                        text, and no guide without Screen Recording
#   scripts/lens-vm.sh screen-words OUT  run lens-screen-words.sh in the VM: a dictation sends
#                                        the screen words it said to the stand-in attn, then
#                                        one across two TextEdit windows sends both's
#   scripts/lens-vm.sh sh COMMAND       run a shell command in the VM's GUI session
#   scripts/lens-vm.sh pull PATH DEST    copy a file or folder out of the VM
#   scripts/lens-vm.sh destroy           stop and delete the VM
set -euo pipefail
cd "$(dirname "$0")/.."

VM=lens-check
TART_VERSION=2.41.1
TART_SHA256=e680a734febccfce7d4e910c1f7f13fc3b00f19a181c945e2d746de3214c32c2
IMAGE=ghcr.io/cirruslabs/macos-tahoe-base@sha256:65c3133a633fda8753cf1f71f2d55727792d534a2125b0032466563dcc8e58fd
# The guest agent answered 35 s after a stop and start on an M5 under load; past this it is broken.
BOOT_LIMIT_SECONDS=180
STATE="$HOME/.tart/vms/$VM"
GUEST_HOME=/Users/admin
GUEST_ROOT="$GUEST_HOME/lens-check"

die() {
    printf 'lens-vm: %s\n' "$*" >&2
    exit 1
}
in_vm() { tart exec "$VM" "$@"; }
in_vm_stdin() { tart exec -i "$VM" "$@"; }
running() { tart list --format json | /usr/bin/python3 -c \
    'import json,sys; sys.exit(0 if any(v["Name"]==sys.argv[1] and v["Running"] for v in json.load(sys.stdin)) else 1)' "$VM"; }

install_tart() {
    command -v tart >/dev/null && return
    # The Homebrew formula does not load on current Homebrew, so use the release.
    local dest="$HOME/.local/share/tart-$TART_VERSION" download
    download="$(mktemp -d)"
    gh release download "$TART_VERSION" --repo cirruslabs/tart --pattern tart.tar.gz --dir "$download"
    printf '%s  %s\n' "$TART_SHA256" "$download/tart.tar.gz" | shasum -a 256 -c -
    mkdir -p "$dest" "$HOME/.local/bin"
    tar xzf "$download/tart.tar.gz" -C "$dest"
    ln -sf "$dest/tart.app/Contents/MacOS/tart" "$HOME/.local/bin/tart"
    command -v tart >/dev/null || die "installed Tart, but ~/.local/bin is not on PATH"
}

start() {
    running && return
    [ -d "$STATE" ] || die "no VM named $VM; run: scripts/lens-vm.sh setup"
    nohup tart run --no-graphics "$VM" >"$STATE/run.log" 2>&1 &
    local deadline=$((SECONDS + BOOT_LIMIT_SECONDS))
    until in_vm /usr/bin/true 2>/dev/null; do
        [ "$SECONDS" -lt "$deadline" ] ||
            die "$VM's guest agent did not answer within ${BOOT_LIMIT_SECONDS}s; see $STATE/run.log"
        sleep 1
    done
    in_vm_stdin /bin/sh -s <tools/vm/guest-ready.sh
}

push() {
    GLANCE_CODESIGN_IDENTITY=- GLANCE_BUNDLE_DEST="$PWD/target/vm/Attn Lens.app" ./scripts/bundle.sh release
    start
    in_vm /bin/sh -c "rm -rf '$GUEST_ROOT' '$GUEST_HOME/Applications/Attn Lens.app'; mkdir -p '$GUEST_ROOT' '$GUEST_HOME/Applications' '$GUEST_HOME/.local/bin'"
    tar cf - scripts/lens-mac-check.sh scripts/lens-live.sh scripts/lens-editor-check.sh scripts/lens-voice.sh scripts/lens-tile.sh scripts/lens-c16.sh scripts/lens-switch.sh scripts/lens-focus.sh scripts/lens-keys.sh scripts/lens-dictation.sh scripts/lens-settings.sh scripts/lens-screen-text.sh scripts/lens-screen-words.sh speech/tests/fixtures tools/driver tools/vm |
        in_vm_stdin tar xf - -C "$GUEST_ROOT"
    tar cf - -C target/vm 'Attn Lens.app' | in_vm_stdin tar xf - -C "$GUEST_HOME/Applications"
    in_vm /bin/sh -c "cd '$GUEST_ROOT' && /usr/bin/swiftc -O tools/driver/driver.swift -o driver && cp tools/vm/attn '$GUEST_HOME/.local/bin/attn'"
    in_vm_stdin /bin/sh -s <tools/vm/guest-grant.sh
}

case "${1:-}" in
setup)
    install_tart
    tart list --quiet | grep -qx "$VM" || {
        tart pull "$IMAGE"
        tart clone "$IMAGE" "$VM"
    }
    start
    ;;
start) start ;;
stop) if running; then tart stop "$VM"; fi ;;
status)
    tart get "$VM"
    if running; then in_vm /bin/sh -c 'uptime; launchctl managername'; fi
    ;;
push) push ;;
driver)
    [ $# -ge 2 ] || die "usage: lens-vm.sh driver SCRIPT [--front APP ...]"
    script="$2"
    shift 2
    start
    guest_script="$GUEST_ROOT/$(basename "$script")"
    in_vm_stdin /bin/sh -c 'cat > "$1"' sh "$guest_script" <"$script"
    if [ $# -eq 0 ]; then set -- --front "Attn Lens"; fi
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm "$GUEST_ROOT/driver" "$@" "$guest_script"
    ;;
check)
    [ $# -eq 2 ] || die "usage: lens-vm.sh check OUT"
    start
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && scripts/lens-mac-check.sh vm-target '$GUEST_ROOT/out'"
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT" out sent | tar xf - -C "$2"
    ;;
live)
    [ $# -eq 2 ] || die "usage: lens-vm.sh live OUT"
    start
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && scripts/lens-live.sh '$GUEST_ROOT/out'"
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT" out sent | tar xf - -C "$2"
    ;;
voice)
    [ $# -ge 2 ] || die "usage: lens-vm.sh voice OUT [FIXTURE]"
    start
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && scripts/lens-voice.sh '$GUEST_ROOT/out' ${3:-short-phrases}"
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT" out | tar xf - -C "$2"
    ;;
c16)
    [ $# -eq 2 ] || die "usage: lens-vm.sh c16 OUT"
    start
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && scripts/lens-c16.sh '$GUEST_ROOT/out'"
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT" out | tar xf - -C "$2"
    ;;
editor)
    [ $# -eq 2 ] || die "usage: lens-vm.sh editor OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf editor-out && scripts/lens-editor-check.sh '$GUEST_ROOT/editor-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/editor-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
tile)
    [ $# -eq 2 ] || die "usage: lens-vm.sh tile OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && scripts/lens-tile.sh '$GUEST_ROOT/tile-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/tile-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
switch)
    [ $# -eq 2 ] || die "usage: lens-vm.sh switch OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf switch-out && scripts/lens-switch.sh '$GUEST_ROOT/switch-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/switch-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
focus)
    [ $# -eq 2 ] || die "usage: lens-vm.sh focus OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf focus-out && scripts/lens-focus.sh '$GUEST_ROOT/focus-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/focus-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
keys)
    [ $# -eq 2 ] || die "usage: lens-vm.sh keys OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf keys-out && scripts/lens-keys.sh '$GUEST_ROOT/keys-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/keys-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
dictation)
    [ $# -eq 2 ] || die "usage: lens-vm.sh dictation OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf dictation-out && scripts/lens-dictation.sh '$GUEST_ROOT/dictation-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/dictation-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
settings)
    [ $# -eq 2 ] || die "usage: lens-vm.sh settings OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf settings-out && scripts/lens-settings.sh '$GUEST_ROOT/settings-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/settings-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
screen-text)
    [ $# -eq 2 ] || die "usage: lens-vm.sh screen-text OUT"
    start
    status=0
    in_vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf screen-text-out && scripts/lens-screen-text.sh '$GUEST_ROOT/screen-text-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/screen-text-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
screen-words)
    [ $# -eq 2 ] || die "usage: lens-vm.sh screen-words OUT"
    start
    status=0
    in_vm /usr/bin/env LENS_DRIVER_WINDOW=vm /bin/sh -c "cd '$GUEST_ROOT' && rm -rf screen-words-out && scripts/lens-screen-words.sh '$GUEST_ROOT/screen-words-out'" || status=$?
    mkdir -p "$2"
    in_vm /usr/bin/tar cf - -C "$GUEST_ROOT/screen-words-out" . | tar xf - -C "$2"
    exit "$status"
    ;;
sh)
    [ $# -eq 2 ] || die "usage: lens-vm.sh sh COMMAND"
    start
    in_vm /bin/sh -c "$2"
    ;;
pull)
    [ $# -eq 3 ] || die "usage: lens-vm.sh pull PATH DEST"
    mkdir -p "$3"
    in_vm /usr/bin/tar cf - -C "$(dirname "$2")" "$(basename "$2")" | tar xf - -C "$3"
    ;;
destroy)
    if running; then tart stop "$VM"; fi
    tart delete "$VM"
    ;;
*)
    sed -n 2,20p "$0" >&2
    exit 2
    ;;
esac
