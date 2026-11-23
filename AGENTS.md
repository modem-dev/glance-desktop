# Working on Attn Lens

Attn Lens captures part of the live screen, marks it up in place, and sends it to
an attn agent session through the `attn` CLI. It is a fork of Glance; the rest of
this file describes Glance's rules, which still apply to the code that remains.

## Lens entry points

Lens is being rebuilt on Rune. `s-24857n` in the attn garden holds the plan. Glance's
GPUI code is at `git show a64f4fb:src/...`, the reference for behavior being ported.
This file's Glance rules still apply to the code that remains.

- `src/app.rs`: the Rune app and its windows. A Dock app (⌘Q quits, a Dock click
  opens the editor). When `rune::tooling::is_headless()` is true, overlays are hosted
  as ordinary windows; that is the only branch on the platform.
- `src/platform_port.rs`: `PlatformPort`, everything the UI asks of the machine.
  `platform_port/mac.rs` is real; `platform_port/fake.rs` is driven by script marks.
- `src/keys/`: the capture key (right ⌘ tapped), the voice key (right ⌥ tapped or
  held) and the draw key (fn), ported from Hex's key engine. `mod.rs` is the
  platform-free tap-or-hold machine and its tests; `tap.rs` is macOS's event taps,
  which need Accessibility. `LENS_KEYS_LOG=1` prints each decision. ⌘⌥C and ⌘⌥↩
  stay Carbon hotkeys in `mac.rs`.
- `src/editor/`: the editor window. Every toolbar, menu, key and gesture reaches
  `Editor::dispatch` in `dispatch.rs`. `gesture.rs` is the pointer on the canvas;
  `preview.rs` renders the raster off the UI thread; `live.rs` paints marks not yet
  in it. `stage.rs` renders the backdrop behind the image, and whole frames while
  the Animation panel plays, on the port's `Lane` (one thread on the Mac, so Metal
  keeps its thread-local state); `export.rs` runs GIF and MP4 export. In scripts,
  MP4 goes through the stand-in encoder `tests/video-encoder`. `tests/editor.rs` runs `headless/editor/*.json`, one script per row of
  `TOOL_OPTIONS.md` and the tables in `docs/usage.md`; each holds its assertions.
- `src/lens/mod.rs`: `Lens`, the state every window and the socket read. Each change
  wakes the windows, because a Rune window does not redraw from another's change.
- `src/lens/session.rs`: Capture, Still, Segment, HostTime.
- `src/lens/select.rs`, `ink.rs`, `bar.rs`: the area picker, the ink layer and the bar.
  `app.rs` hosts the picker, the ink layer and the four corner handles as `Surface`s.
  The ink layer takes the pointer only while the draw key is held; the handles are
  small windows of their own, below the menu bar, that hold still during a resize.
- `src/lens/tile.rs`: the corner tile a closed capture shrinks to. While the editor
  holds a still (`State::editing`), the live surfaces hide and the watch stops;
  `EditorCall` is how Lens reaches the editor, and `Editor::leave_still` gives it back.
- `src/lens/attn.rs`: the only contract with attn: `attn send sessions`, `attn
  send pair`, `attn send <session>` and `attn model call` JSON, plus the Keychain token. Its
  tests use a stand-in `attn`; attn's `cmd/attn/send_stack_test.go` and
  `cmd/attnd/model_call_stack_test.go` pin the real JSON.
- `src/lens/dictation.rs`, `pill.rs`: the voice key with no capture open: the pill,
  the refinement (`attn model call` with `src/lens/refiner.rs`'s request; its corpus
  is `probes/refine`, `cargo run --example refine-probe`), the one paste
  (`src/paste.rs`, from Hex, behind `PlatformPort::paste`) and the hand-over to a
  capture. See `docs/dictation.md`. `LENS_DICTATION_LOG=1`
  prints each step's time; `--pill-in-capture` lets a recording show the pill.
- `src/kit.rs`: `kit(dark)`, the one rune-kit `Kit`, painted with attn's default scheme
  (values copied from attn, with the file and commit named above them). Views take
  colours, type, radii and shadows from it; `src/editor/panels/controls.rs` wraps the
  kit components the sidebar uses, and a Select's list mounts through `rune_kit::root`
  in `editor::view`. The capture surfaces (bar, palette, tile, pill, the strip's chips)
  paint with `kit::hud()`, the dark kit in every appearance, edged with `hud_edge()`.
- `src/settings.rs`: `Settings`, Lens's settings, in `prefs.json` under `dev.attn.lens`
  through `rune::prefs`. A new setting is one line in its `settings!`: field, key and
  default. Lens holds them in `State.settings`; only `Lens::set_settings` changes them.
  Headless runs set `RUNE_PREFS_DIR` (`tests/headless.rs` gives each run its own).
- `src/settings_window.rs`: the Settings window, built from rune-kit: sections of rows,
  each a label, one line on what it does, and its control. "Settings…" (⌘,) in the app
  menu and in the Dock menu (`src/editor/menus.rs`) call `Lens::open_settings`.
- `src/lens/deliver.rs`: what Send delivers: the timeline message, the contact
  sheets and the frames, written only from `Document::export()`.
- `src/lens/capture.rs`: stills through Hex's capture-core (ScreenCaptureKit), and
  `front_window()`, the topmost window that is not Lens's, by z-order. `watch.rs`'s
  `front_windows` (`PlatformPort::front_windows`, which the capture watch uses for window
  switches) polls it, and `PlatformPort::screen_text(window)` reads a window by its
  number (its still, then OCR); the automation socket's `screen_text` request reads the
  front one.
- `src/lens/screen_words.rs`: screen words for dictation refinement, pure: `pick` keeps
  the screen read's words worth spelling, `matching` the ones the transcript could mean,
  `sound_pairs` the screen words that sound like transcript words. The rules are the
  Design tables of `s-0ww1ak`; each rule and the prototype's corpus
  (`tests/fixtures/screen-words/`) is a table row. `dictation.rs` reads the front window as a
  dictation starts and each new one `PlatformPort::front_windows` reports until voice
  stops, once each (`PlatformPort::screen_text`, while the setting is on), and calls them
  over the reads' text joined when it refines; `headless/dictation-screen-words.json`
  and `dictation-screen-words-switch.json` (the fake's `window:` marks) hold what reaches
  the input.
- `src/lens/watch.rs`: macOS's `PlatformPort::watch` and `PlatformPort::front_windows`. `LENS_WATCH_LOG=1` prints each
  event, and `--ink-in-capture` lets a recording of a driven check show the ink.
- `src/lens/voice.rs`: voice. It runs `speech/` (attn's speech helper, protocol 3)
  as a child, feeds it the microphone (`voice/mic.rs`) or, through the socket's
  `voice-wav`, a WAV, and puts each phrase on the capture clock. `speech/` is its
  own crate, built by `scripts/bundle.sh`; `speech/tests/segments.rs` and
  `tests/voice.rs` check phrase times against `speech/tests/fixtures` (opt in with
  the models, as their headers say). `scripts/lens-vm.sh voice OUT` drives it in the VM.
- `src/lens/automation.rs`: the development socket. Launch with `--automation`, then
  `AttnLens lens '{"cmd":"state"}'`; the module's header lists the requests.
- `tools/driver/driver.swift`: posts synthetic keys and drags. Run it in the VM below;
  on a real Mac only in a window agreed with the user. It stops if real input arrives.
- `scripts/lens-vm.sh`: a macOS VM (Tart) for driven checks, so no key reaches this Mac.
  `setup` once, then `push` to install this checkout signed ad hoc, `driver SCRIPT` to run
  a driver script in the VM's GUI session, `check OUT` to run `scripts/lens-mac-check.sh`
  and `live OUT` to run `scripts/lens-live.sh`, both against the stand-in `tools/vm/attn`,
  `editor OUT` to run `scripts/lens-editor-check.sh` (menus, Copy as OCR, Pick from screen,
  scroll and zoom, light and dark, read through `tools/driver/ax.swift`),
  `tile OUT` to run `scripts/lens-tile.sh` (tile hover and click, and Back not counted as a change),
  `switch OUT` to run `scripts/lens-switch.sh` (draw, ⌘Tab, draw, ⌘Tab, draw: fn read while another app is in front),
  `keys OUT` to run `scripts/lens-keys.sh` (right ⌘ and right ⌥ alone and in chords with TextEdit, and the Accessibility guide),
  `dictation OUT` to run `scripts/lens-dictation.sh` (dictation into TextEdit with focus and clipboard kept, and its timings),
  `screen-words OUT` to run `scripts/lens-screen-words.sh` (a dictation with TextEdit showing an identifier sends it in `screen_words`, and one moving between two TextEdit windows sends both windows' identifiers),
  `c16 OUT` to run `scripts/lens-c16.sh` (the 60 s drawing, voice and stills capture; `scripts/lens-c16-linux.sh`
  is its headless Linux half), and `stop` when done: it holds 8 GB. `tools/vm/`
  grants the VM's permissions without a click. Nothing of this Mac's goes in: no shared
  folders, Keychain, tokens or signing identity. Speed budgets and the feel of drawing
  stay on the real Mac; VM numbers do not count for them.
- `headless/`: Rune scripts, asserted by `tests/headless.rs`. Assert on trees, read with
  `--window KEY --tree out.json --at mark:NAME`, not only on snapshots.
- `src/editor/scenes.rs`, `src/lens/scenes.rs`: each window's scenes, one per screen and
  hard state (`--window KEY --list-scenes`). A scene builds its state through the actions,
  gestures and script marks the app and its scripts use, never by setting fields.
  Capture scenes list sessions through the stand-in `tools/scenes/attn`. A state that
  needs a job held in flight (sending, refining) plays the fake port's `jobs:hold` mark
  first. A new screen or state adds its scene beside its code.
- `scripts/snap.sh OUT [WINDOW...]` renders every scene of the named windows (all by
  default) in dark and light, into `OUT/WINDOW--SCENE--THEME.png` and the contact sheets
  `OUT/dark.png` and `OUT/light.png`. `SIZE=900x560` renders the editor at its smallest.
  `DESKTOP=tools/scenes/desktop-light.png` (or `-dark.png`) puts the capture surfaces
  over a light or a dark screen in place of the editor's fixture.

## Look at the render before you report UI work done

A change to how a window looks or lays out is not done until you have seen it. Run
`scripts/snap.sh OUT WINDOW...` for each window the change touches, then again with
`SIZE=900x560` when it touches the editor. Open the sheets and the PNGs of the scenes
you changed, in both themes, and compare them with the same scenes on `main`. Attach the
sheets to the seed or the pull request. To check a colour or a rect rather than eyeball
it, read the tree: `attn-lens --window KEY --scene NAME --at 1 --tree out.json`, with the
flags `snap.sh` passes for that window. Each element's entry, found by its `path`
(`bar/#1/send`), has its `fill` and `rect`.
If no scene shows what you changed, add one.

## More entry points

- `src/permission_guide.rs`, `native/permission_guide.m`: from Hex (MIT).
- Sign dev bundles with a stable identity (`GLANCE_CODESIGN_IDENTITY`): an
  ad-hoc rebuild silently invalidates the Screen Recording grant.
- `scripts/install.sh` builds this checkout, replaces
  `~/Applications/Attn Lens.app`, quits the running copy and opens the new one.
  It signs with this Mac's Apple Development identity unless
  `GLANCE_CODESIGN_IDENTITY` names another, and refuses ad-hoc. It replaces the
  user's installed app, so run it only when asked to install.

# Glance

Glance was a native macOS screenshot editor. Its image model remains: one document,
exported to PNG, GIF and MP4. Use Cargo and the existing Rust tooling; do not add a
JavaScript toolchain.

## Entry points and architecture

- `src/document.rs` and `document/actions.rs`: image model, validated edits, undo.
- `src/arrow.rs`, `backdrop.rs`, `effects.rs`, `style.rs`, `selection.rs`,
  `navigation.rs`: mark geometry and raster painting, free of any UI toolkit.
- `src/animation.rs`, `src/motion_shader.rs`, `src/shaders/motion.metal`: backdrops
  and entrance animation for export.
- `src/platform.rs`: dialogs, fonts, and clipboard; Linux lives in `src/platform/linux.rs`.
- `native/`: AVFoundation/FFmpeg helpers. `scripts/bundle.sh`: macOS packaging.

See `docs/usage.md` for the editor's behavior.
`PLAN.md` is historical; do not use its early scope as the current feature list.

## Working rules

- Keep rendering/export work off the UI thread. Preserve undo and editable
  annotations; guard asynchronous results with operation/revision checks.
- Dispatch typed actions from new UI or automation triggers. Do not fabricate
  keyboard events to invoke commands.
- Keep local editing offline. Do not upload captures, expose the automation
  bridge, or change certificate trust as an incidental validation step.
- Use synthetic samples for docs and tests. Preserve third-party licenses and
  provenance. Keep signing material and credentials outside Git.
- Keep PRs focused and update usage docs when behavior or shortcuts change.

## File and process safety

- Use private, randomized temporary directories for captures and helper outputs.
  Keep export scratch files beside the destination for atomic rename. Never use
  predictable PID/counter paths or truncate an unreserved temporary file.
- Preserve the exact filename confirmed by a save dialog. An appended extension
  must not silently replace an existing file or symlink. Automation exports create new
  destinations and must reject existing files.
- Treat automation media inputs as local files: reject URLs and special files,
  bound actual reads, and disable network protocols in media helpers. Pass
  arguments directly to processes; never interpolate them into shell scripts.
- Check dependency advisories during security reviews. Distinguish affected
  versions from reachable vulnerable APIs; record evidence and upstream limits.

## Validation

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

For native-helper or packaging changes, build with `./scripts/bundle.sh debug`.
For CI/isolated packaging checks, set `GLANCE_CODESIGN_IDENTITY=-` to avoid
certificate trust setup. Ad-hoc bundles have unstable capture permissions;
do not replace a user's installed app with one as part of validation.

Check `QA.md` for manual desktop verification.
Ignored tests are opt-in and may write media, use native helpers, benchmark, or
upload to production; read the test before explicitly running it.

## Changelog and releases

Maintain top-level `CHANGELOG.md` for user-visible changes. Add entries to the
existing `Added`, `Changed`, or `Fixed` subsection of `Unreleased`; do not create
duplicate headings. Omit internal-only refactors unless behavior changes.

On release, move the relevant entries into a dated version section, leave that
released section immutable, and start a fresh `Unreleased` section. Align Cargo
and bundle versions. Use the released section as the GitHub release body and
verify any generated notes. Use `gh release create/edit --notes-file` for
multiline notes. Publishing releases or changing visibility requires a user
request; preparing the repository does not make it public.

Commit titles use Conventional Commits: `<type>[scope]: <description>`.
