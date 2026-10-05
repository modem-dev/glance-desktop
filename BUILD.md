# Building Glance

For a ready-to-run app, use the [release downloads](https://github.com/modem-dev/glance-desktop/releases/latest).
The instructions below build from source using Rust and Cargo.

## macOS

Requires macOS 12+, Xcode Command Line Tools, and stable Rust from
[rustup](https://rustup.rs/). Development is verified on Apple Silicon.
The bundle includes AVFoundation video helpers.

```sh
xcode-select --install # If Command Line Tools are missing.
git clone https://github.com/modem-dev/glance-desktop.git
cd glance-desktop
./scripts/bundle.sh
```

The first build creates a persistent local signing identity. If it stops with
a certificate-trust message, review the setup script, run it once, and rebuild:

```sh
./scripts/trust-local-signing.sh
./scripts/bundle.sh
```

This adds user-Keychain trust for the development certificate for code signing.
The certificate and keychain live in
`~/Library/Application Support/Glance/Signing`; keep that directory across rebuilds.

Launch the bundle directly:

```sh
open target/Glance.app
```

To make it available through Spotlight or Raycast, link it into your Applications folder:

```sh
mkdir -p ~/Applications
ln -s "$(pwd)/target/Glance.app" ~/Applications/Glance.app
open ~/Applications/Glance.app
```

If that destination already exists, choose whether to keep it or replace it before
creating the link. Rebuilding updates the linked app; quit and reopen to use it.

### Capture permissions

On first capture, grant Glance **Screen Recording** in **System Settings →
Privacy & Security** (called **Screen & System Audio Recording** on newer macOS),
then quit and reopen. On macOS 12, use **System Preferences → Security & Privacy → Privacy**.
Use the same bundle path and signing identity for later builds.

If capture fails despite an enabled grant, quit Glance, remove its entry with **−**,
add your linked `Glance.app` with **+**, enable access, and reopen.
Switching signing identities may require this again.

### Build modes and signing

Use `./scripts/bundle.sh debug` for a faster bundle. `cargo run --locked -- desktop` is useful
for UI iteration; run the bundle build once to compile the native video helpers.
The packaged app gives capture permissions a consistent identity.

To use an existing code-signing certificate:

```sh
GLANCE_CODESIGN_IDENTITY="Your code-signing certificate name" ./scripts/bundle.sh
```

Keep the same certificate across builds. For an isolated packaging check, use a
separate destination and ad-hoc signing:

```sh
GLANCE_BUNDLE_DEST="$(pwd)/target/packaging-check/Glance.app" \
  GLANCE_CODESIGN_IDENTITY=- ./scripts/bundle.sh debug
```

Ad-hoc signing can invalidate capture grants after rebuilding. Keep packaging
checks separate from your installed app. Local development signing and release
notarization are separate processes; see [release procedures](docs/development.md#releases).

## Omarchy / Arch Linux

Install stable Rust through rustup, or use Arch's Rust package with these dependencies:

```sh
sudo pacman -S --needed base-devel rust clang cmake \
  fontconfig freetype2 libx11 libxcb libxkbcommon libxkbcommon-x11 \
  wayland vulkan-icd-loader xdg-utils ttf-dejavu grim slurp wl-clipboard zenity ffmpeg
git clone https://github.com/modem-dev/glance-desktop.git
cd glance-desktop
./scripts/package-linux.sh
cd target/dist
makepkg
sudo pacman -U ./glance-desktop-*.pkg.tar.zst
```

Run `makepkg` as your normal user. The generated PKGBUILD verifies the archive's
SHA-256 and installs both video helpers beside the executable. Local packaging
uses the host architecture; published packages target x86_64.
A working Vulkan GPU driver is required.

For UI iteration, run `cargo run --locked -- desktop`. Copy `native/linux/glance-video-*`
to `target/` once to enable video helpers in Cargo builds.
See the [Omarchy guide](docs/linux.md) for capture bindings and runtime troubleshooting.

## Checks

Run the checks in [CONTRIBUTING.md](CONTRIBUTING.md#make-a-pull-request).
[Development](docs/development.md) covers hooks, CI, and releases;
[QA](QA.md) covers desktop acceptance and opt-in media tests.
