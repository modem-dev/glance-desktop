#!/bin/sh
# Local development signing: keep the certificate outside target/ and the repo.
# Trust setup is a separate explicit step; no privacy database edits are made.
set -eu
umask 077
APP="${1:?Usage: scripts/sign-app.sh /path/to/Glance.app}"
SIGN_DIR="${GLANCE_SIGNING_DIR:-$HOME/Library/Application Support/Glance/Signing}"
SIGN_IDENTITY="${GLANCE_CODESIGN_IDENTITY:-}"
# Reuse the existing certificate during the app rename. Replacing it would
# discard code-signing trust and change the identity on every existing grant.
if [ -z "${GLANCE_SIGNING_DIR:-}" ] && [ ! -e "$SIGN_DIR" ] && [ -d "$HOME/Library/Application Support/Pachiri/Signing" ]; then
    mkdir -p "$(dirname "$SIGN_DIR")"
    mv "$HOME/Library/Application Support/Pachiri/Signing" "$SIGN_DIR"
    printf 'Migrated the existing local signing identity to %s\n' "$SIGN_DIR"
fi
KEYCHAIN="$SIGN_DIR/local-signing.keychain-db"
TEMP_DIR=""
LOCAL_SIGNING=false
SEARCH_LIST=""
restore_search_list() {
    if [ -n "$SEARCH_LIST" ]; then
        set --
        while IFS= read -r path; do set -- "$@" "$path"; done < "$SEARCH_LIST"
        security list-keychains -d user -s "$@"
        SEARCH_LIST=""
    fi
}
cleanup() {
    restore_search_list
    if [ "$LOCAL_SIGNING" = true ]; then security lock-keychain "$KEYCHAIN" >/dev/null 2>&1 || :; fi
    if [ -n "$TEMP_DIR" ]; then rm -rf "$TEMP_DIR"; fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

if [ -z "$SIGN_IDENTITY" ]; then
    LOCAL_SIGNING=true
    mkdir -p "$SIGN_DIR"
    chmod 700 "$SIGN_DIR"
    if [ ! -f "$KEYCHAIN" ]; then
        # Existing metadata without its keychain must not silently get a new key.
        if [ -f "$SIGN_DIR/identity" ] || [ -f "$SIGN_DIR/password" ]; then
            printf 'Local signing keychain is missing; restore %s instead of replacing its identity.\n' "$SIGN_DIR" >&2
            exit 1
        fi
        TEMP_DIR="$(mktemp -d "$SIGN_DIR/setup.XXXXXX")"
        /usr/bin/openssl rand -hex 32 > "$SIGN_DIR/password"
        cat > "$TEMP_DIR/certificate.cnf" <<'CONFIG'
[req]
prompt = no
distinguished_name = subject
x509_extensions = codesign
[subject]
CN = Glance Local Development
O = Glance
[codesign]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CONFIG
        /usr/bin/openssl req -new -newkey rsa:2048 -nodes -x509 -days 3650 \
            -config "$TEMP_DIR/certificate.cnf" \
            -keyout "$TEMP_DIR/private.pem" -out "$TEMP_DIR/certificate.pem" 2>/dev/null
        /usr/bin/openssl pkcs12 -export -name 'Glance Local Development' \
            -inkey "$TEMP_DIR/private.pem" -in "$TEMP_DIR/certificate.pem" \
            -out "$TEMP_DIR/identity.p12" -passout "file:$SIGN_DIR/password"
        # create-keychain changes the search list; restore it immediately.
        SEARCH_LIST="$TEMP_DIR/search-list"
        security list-keychains -d user | sed 's/^[[:space:]]*"//;s/"$//' > "$SEARCH_LIST"
        security create-keychain -p "$(cat "$SIGN_DIR/password")" "$KEYCHAIN"
        restore_search_list
        security set-keychain-settings -l "$KEYCHAIN"
        security unlock-keychain -p "$(cat "$SIGN_DIR/password")" "$KEYCHAIN"
        security import "$TEMP_DIR/identity.p12" -k "$KEYCHAIN" -f pkcs12 \
            -P "$(cat "$SIGN_DIR/password")" -x -T /usr/bin/codesign >/dev/null
        security set-key-partition-list -S 'apple-tool:,apple:' -s \
            -k "$(cat "$SIGN_DIR/password")" "$KEYCHAIN" >/dev/null
        /usr/bin/openssl x509 -in "$TEMP_DIR/certificate.pem" -noout -fingerprint -sha1 \
            | sed 's/.*=//;s/://g' > "$SIGN_DIR/identity"
        cp "$TEMP_DIR/certificate.pem" "$SIGN_DIR/certificate.pem"
        printf 'Created persistent Glance development identity in %s\n' "$SIGN_DIR"
    fi
    if [ ! -f "$SIGN_DIR/password" ] || [ ! -f "$SIGN_DIR/identity" ]; then
        printf 'Local signing setup is incomplete in %s; refusing to replace its key.\n' "$SIGN_DIR" >&2
        exit 1
    fi
    SIGN_IDENTITY="$(cat "$SIGN_DIR/identity")"
    if ! security find-identity -v -p codesigning "$KEYCHAIN" | /usr/bin/grep -Fq "$SIGN_IDENTITY"; then
        printf 'Glance local certificate needs code-signing trust. Review and run scripts/trust-local-signing.sh once, then retry.\n' >&2
        exit 1
    fi
    security unlock-keychain -p "$(cat "$SIGN_DIR/password")" "$KEYCHAIN"
    # codesign still needs this search-list entry even with --keychain. Restore
    # the exact previous list in the exit trap, including on a signing failure.
    if [ -z "$TEMP_DIR" ]; then TEMP_DIR="$(mktemp -d "$SIGN_DIR/sign.XXXXXX")"; fi
    SEARCH_LIST="$TEMP_DIR/search-list"
    security list-keychains -d user | sed 's/^[[:space:]]*"//;s/"$//' > "$SEARCH_LIST"
    set --
    while IFS= read -r path; do set -- "$@" "$path"; done < "$SEARCH_LIST"
    security list-keychains -d user -s "$@" "$KEYCHAIN"
fi

sign() {
    if [ "$LOCAL_SIGNING" = true ]; then
        codesign --force --timestamp=none --keychain "$KEYCHAIN" --sign "$SIGN_IDENTITY" "$1"
    else
        codesign --force --sign "$SIGN_IDENTITY" "$1"
    fi
}
# Sign nested executable first, then seal the bundle, rather than using --deep.
sign "$APP/Contents/MacOS/glance-video-encoder"
sign "$APP/Contents/MacOS/glance-video-frame"
sign "$APP/Contents/MacOS/glance-ocr"
sign "$APP"
codesign --verify --deep --strict "$APP"
if [ "$SIGN_IDENTITY" = '-' ]; then
    printf 'Explicit ad-hoc signing: rebuilding can invalidate Screen Recording permission.\n'
else
    # Catch accidental loss of the stable certificate-based requirement.
    REQUIREMENT="$(codesign -d -r- "$APP" 2>&1)"
    if printf '%s\n' "$REQUIREMENT" | /usr/bin/grep -q 'designated => cdhash'; then
        printf 'Signing produced a build-specific requirement; refusing this bundle.\n' >&2
        exit 1
    fi
    printf 'Signed with a persistent certificate; identity is stable across rebuilds.\n'
fi
