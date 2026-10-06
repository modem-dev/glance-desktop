# Changelog

Notable user-visible changes are recorded here.

## [Unreleased]

### Added

- Optional **Continue with ChatGPT** sign-in, saved account/model selection, and
  subscription-powered OCR through the existing Copy as OCR button, with usage
  controls, private credential storage, and matching MCP actions.
- **Copy as OCR** in the top toolbar recognizes screenshot text offline and copies
  it directly to the clipboard: Apple Vision on macOS, optional Tesseract on
  Omarchy, and a matching MCP action with source-region selection and read-back.

### Changed

- Reorganized documentation around release installation and first use, added a
  macOS opening/capture FAQ, and consolidated source-build instructions in BUILD.md.
- README includes a short album-gallery GIF showing rounded padding and an
  animated Lava backdrop.

## [0.3.0] - 2026-10-04

### Changed

- Select mode supports dragging a selection box, Shift-click multi-selection and
  ⌘A to select all, with group dragging, styling, nudging, duplication and deletion.
- Animation sidebar now focuses on foreground effects and playback; backdrop
  controls stay in Backdrop and export stays in the toolbar menu. Toolbar icons
  are ordered Backdrop, Animation, then Image tools.
- Color pickers now offer a single “Pick from screen” button with an eyedropper
  icon, replacing the separate image and screen sampling buttons.

### Fixed

- Animation previews show a loading cover while preparing the first frame or
  seeking, and hold playback so startup does not skip the entrance or flash
  incomplete frames.
- Foreground animations over moving backdrops use cached Metal sampling on macOS,
  reuse the prepared card during effect/timing edits, and avoid blank flashes
  while preview quality changes or the combined preview starts.
- MP4/GIF exports now show a persistent progress bar, percentage and cancel button
  at the bottom of the window, including when the inspector is closed or scrolled.
- Animation entrance duration and delay labels no longer overlap their values
  in the compact sidebar.
- macOS accessibility now exposes toolbar and inspector controls, current values,
  color hex editing, menus and canvas dimensions. Accessible edits use the shared
  actions and reject stale targets.

## [0.2.0] - 2026-10-04

### Added

- Randomize button for all eight backdrop motions, with undoable, reproducible
  seeds shared by preview, PNG/GIF/MP4 exports and MCP, while retaining seamless loops.

- Reusable backdrop color popup with a color wheel, brightness, hex entry and
  image/screen eyedroppers; one solid color or two gradient/motion colors, with
  undo and matching PNG/GIF/MP4 output and MCP controls.

- Image Animation sidebar with diagonal reveal, spring pop, and 3D settle;
  independent backdrop motion, replay/scrubbing, and optional exits in MP4/GIF.

- Contextual sidebar options for all annotation tools, with separate defaults
  remembered during the session and undoable edits to selected annotations.
- Dashed and dotted lines, independent arrow/dot ends, editable waypoints,
  filled and rounded boxes, and reversible Raw/Smooth/Adaptive pen cleanup.
- Text sizing and opacity, highlight intensity, pixelation block size, crop
  aspect ratios, step numbers, spotlight dimming, and magnifier defaults.

### Changed

- Removed the “New annotation” label from tool inspector headers.
- Tool shortcuts and brief instructions now appear in a hover tooltip on the
  help icon beside each tool's name, replacing the inspector footer hints.

- All tool inspectors use compact paired fields, editable numeric values, visual
  stroke/fill/endpoint choices and a shared custom color picker. Backdrop, Image
  tools and Animation use the same spacing and compact controls; annotation
  color sampling preserves opacity and is also available through MCP.

- Starfield is now called Nebula, with subtle evolving gas clouds, soft
  filaments and dark dust lanes behind the prominent drifting stars.

- Motion backdrop buttons have distinct small icons beside each effect name.

- Backdrop color controls use compact swatches, keep all eight presets on one
  row, and give Play/Pause an outline aligned with the duration slider.

- Lava backdrops flow through crossing currents, with stretching molten shapes
  and small globules that separate and rejoin the larger streams.

- Prism backdrops have irregular crystal facets, dimensional lighting and
  sweeping bands of refracted color, with subtler illuminated edges.

- Replaced backdrop canvas corners with inside padding that extends screenshot
  edge pixels, with matching image corners, shadows, preview and PNG/GIF/MP4 output.

- Linux release builds reuse optimized dependency libraries cached on `main`
  across release tags, alongside the existing CI test cache.

### Fixed

- Capture and animation-export scratch files use private temporary directories,
  preventing local symlink attacks and exposure of unedited captures.
- Save dialogs preserve overwrite confirmation when checking filename extensions.
- MCP media reads reject remote URLs and special files; image imports enforce
  their size limit while reading, and Linux video decoding disables network protocols.

- Resized annotation widths and corners stay editable and round-trip through MCP;
  invalid transformed geometry is rejected before changing the image or undo history.
- Zero-length arrows retain their live stroke in exports, and incompatible backdrop
  durations return an action error before changing an image entrance.
- Text exports preserve glyph overhangs such as the left edge of “j”.

- MCP action discovery includes backdrop format and enable/disable controls;
  animation export descriptions reflect image entrances over still or absent backdrops.

- Translucent annotation exports preserve opaque image alpha and blend text
  and stroke joints consistently.

## [0.1.0] - 2026-10-03

Initial release of Glance, a native screenshot editor for annotation, animated
backdrops, and sharing visual context with coding agents.

### Added

- Native macOS area and main-display capture, editable annotations, crop,
  resize, rotation, clipboard import/export, and PNG save.
- Spotlight and magnifier tools, solid and gradient framing, aspect-ratio
  presets, and eight animated backdrops with MP4 and looping GIF export.
- Temporary image sharing through glance.sh and an opt-in local MCP companion.
- MIT license, contributor and security guides, and macOS build validation.
- Experimental Omarchy/Hyprland support with Wayland capture and clipboard,
  Linux dialogs/fonts, Ctrl shortcuts, and FFmpeg video helpers.
- Release-triggered macOS ARM64 ZIP and Omarchy x86_64 Arch package builds with
  checksums attached to GitHub releases; regular CI includes video encode/decode
  checks.
- `--open`, `--capture-area`, and `--capture-screen` startup options.

### Changed

- Reorganized the README around trying the app and sharing visual context, with
  separate usage, development, and architecture guides.

### Known limitations

- The macOS download supports Apple Silicon on macOS 12+. It is ad-hoc signed
  and not notarized; macOS may block opening it. Source builds use a persistent
  local development signing identity. Developer ID signing and Intel/universal
  downloads are not configured.
- Omarchy/Hyprland support is experimental and still needs a real desktop
  acceptance pass. Animated Linux backdrops render on the CPU.
- Save or copy before replacing an image or quitting; editable sessions are
  not persisted. OCR, scrolling capture, and automatic updates are not available.
