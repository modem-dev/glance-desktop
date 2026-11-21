# Changelog

Notable user-visible changes are recorded here.

## [Unreleased]

### Added

- **Attn Lens** dictation uses the words on screen: it reads the window in front
  as you start, and the names and identifiers you said go to the refiner, so they
  come out spelled right, with words on screen that sound like a misheard one.
  Settings › Use words on screen turns it off; without Screen Recording the
  dictation runs as before.
- **Attn Lens** has a Settings window: **Settings…** (⌘,) in the app menu or the
  Dock menu. Its first setting, Dictation › Use words on screen, is on by default
  and is saved at once in `~/Library/Application Support/dev.attn.lens/prefs.json`.
- **Attn Lens**: drag the capture bar by its background to move it off what you
  want to see or click. It stays whole on the capture's display, leaves the
  keyboard where it was, and keeps its place until the capture ends.
- **Attn Lens** dictation: with no capture open, right ⌥ (tap, or hold) dictates
  into the app in front. A pill above the Dock shows the words, a model on attn's
  standard tier refines them through `attn model call` with Lens's own
  instructions, and the text pastes once at the cursor with the clipboard put back. ↩ pastes the raw text at once, Esc cancels; a failed, late or
  unreachable refinement pastes the raw text and says why. Right ⌘ during a
  dictation turns it into a capture's voice. The Dock menu has Copy Last Dictation.

- A capture's stills open in the editor from Edit or a thumbnail. Back returns to
  the same live capture with the edit kept and undoable, and Send works from the
  editor. After Send the capture shrinks to a corner tile that leaves after 5 s,
  stays while hovered and reopens on click; after Esc its tile stays.
- Attn Lens's Send now delivers one message with a timeline of stills, marks and
  spoken words in time order, a numbered contact sheet per six stills, and every
  still as a full-resolution PNG. One still with no speech sends one PNG and a
  short message.
- The editor is back on Rune: every tool and option, selection, undo, resize,
  rotate and smart upscale, the color picker with Pick from screen, Copy as OCR,
  inline text with input methods, native menus and accessibility. Images past 8192
  pixels view downscaled and still save at full size. Pan with Space-drag or the
  middle button; right-drag no longer pans.
- The editor's Backdrop and Animation panels are back on Rune: formats, padding,
  corners, shadow, solid, gradient and moving backgrounds, entrance effects with
  Hold or Exit, playback, and GIF and MP4 export with progress and Cancel.
- **Attn Lens**: a tap of right ⌘ alone, the capture key, opens a crosshair over the live screen: drag for an area,
  click for the whole display, ↩ for the last area, Esc to cancel. The area stays
  outlined for the whole capture, its corners adjust it, and the page under it
  keeps working. Hold fn and drag to draw. While fn is down, a tool strip by the pointer
  shows Pen, Arrow, Box, Highlight, Label and Counter, each drawn as the ink it
  makes; click one or press fn+1 to fn+6 to pick it, and it stays picked. ⇧ makes
  a box with Pen or Arrow. Type a message in the bar, or click the page to give
  it the keys and ⌘⌥C to take them back. The capture key takes a still, × drops one, Tab opens a
  palette of sessions, and ↩ or ⌘⌥↩ sends to an attn session, local or on an
  outpost. Lens pairs with attn once and keeps the token in the Keychain.
- Ink fades on its own when the screen moves under it: a scroll, a click, a window
  switch, or most of the area changing. Clicks and window switches each take a still
  first, and a send takes one more if the screen moved after the last.
- Voice in Attn Lens: during a capture, tap right ⌥ to talk and again to stop, or
  hold it and talk until you let go. What you say is transcribed on this Mac,
  phrase by phrase, each with the time it was spoken. A key pressed with right ⌘
  or right ⌥ (right ⌘+C, right ⌥+e) is left to the app in front. The first voice
  downloads the speech model (731 MB) with its progress in the bar, which then
  shows the model loading, the microphone starting and listening. Send waits for
  the sentence you are finishing. Missing Microphone access opens System Settings.
- Missing Screen Recording access opens System Settings at launch and on the capture key,
  with a tile of the app to drag into the list (from Hex, MIT). The guide says Lens
  is being set up, lists each access with why it is needed, and ends with quitting
  and reopening Lens. Missing Accessibility, which the capture, voice and draw keys
  need, opens the same guide at the Accessibility list.
- The editor interface follows the system’s light or dark appearance, including
  changes while Glance is running. Image content and exported media keep their colors.

### Changed

- **Attn Lens**'s capture bar, session palette, corner tile, dictation pill and tool
  strip are a dark heads-up display in attn's colours, in light and dark appearance
  alike, with a brighter edge so they stand apart from dark windows too. Send is the
  bar's one highlighted button and reads in light appearance, the session chip no
  longer runs under the message field, voice's progress no longer covers Edit, and
  an error reads in red.
- The editor's toolbar and window frame are rune-kit's: the window buttons sit in the
  middle of the toolbar, the tools and actions are kit icon buttons whose tooltips
  name their shortcuts, and the active tool and open panel read as pressed to a
  screen reader. Backdrop and Image tools have their own icons. Zoom moved to the
  status line, and "Copied!" floats under the copy buttons, so the toolbar fits the
  smallest window with Back and Send.

- **Attn Lens**'s editor sidebar and Settings window use attn's colours, and their
  controls come from rune-kit: number steppers, sliders, colour swatches, segmented
  choices and buttons. A choice of more than three options (aspect ratio, image entrance,
  backdrop effect, resize) is a pop-up menu. The sidebar and the editor's default
  window are 12 points wider, so a three-way choice reads whole. A number still
  opens for typing when you click it.
- **Attn Lens**'s Backdrop, Animation and Image tools panels and the colour picker
  follow: Enable backdrop is a switch, Smart upscale a checkbox, the Start and End
  arrowheads pop-up menus, and the hex field reads in light mode and turns red when
  the colour does not parse.
- Attn Lens has a new app icon: attn's cat holding a magnifying glass, in place
  of Glance's pixel eye.

- Attn Lens now runs on Rune instead of GPUI. A tap of right ⌘ replaces ⌘⌥2 and starts a capture
  of the display under the pointer; a Dock click opens the editor. The in-place
  overlay and the full editor return in later changes.
- Removed Ask Glance, glance.sh share links, the MCP server and the terminal commands.
- Stills are taken in-process through ScreenCaptureKit instead of the
  `screencapture` tool, which avoids macOS 26's private window picker consent.
  The area selector is gone; crop in the editor. Requires macOS 15.
- The app is Attn Lens (`dev.attn.lens`), and no window opens at launch.

### Fixed

- Sending from the editor, like every other way a capture ends, gives the keyboard
  back to the app you were in, so you can keep typing without a click.
- The editor's toolbar starts after the window buttons, so close, minimize and zoom
  no longer cover Back and the title.
- Attn Lens: Esc, Enter and Tab in the bar work after you click away and back,
  without clicking the message field first.
- Attn Lens's session chip cuts a long session label with "…" inside the chip and
  keeps the host readable. The Tab palette shows states in words ("waiting for
  you", "needs approval") and an outpost's name, or "outpost" and a short id
  when attn has no name for it, instead of raw states and the full id.
- The app menu reads "Attn Lens", from the bundle, rather than the executable's name.
- Attn Lens: a drag started right after the area picker appears now always registers,
  and captures no longer keep about 10 MB of memory each after they close.

## [0.5.0] - 2026-10-07

### Added

- **Ask Glance** prompt bar (⌘K / Ctrl+K): use your ChatGPT plan to inspect,
  crop, resize, annotate, redact with pixelation, and style a screenshot through
  a native agent loop. Completed edits apply together with one undo; Stop, failed
  requests, and stale results discard the draft. Includes model selection,
  answer copying, and matching MCP actions/state.
- Optional **Continue with ChatGPT** sign-in, saved account/model selection, and
  subscription-powered OCR through the existing Copy as OCR button, with usage
  controls, private credential storage, and matching MCP actions. The compact
  account panel uses dropdowns, labeled icon actions, and a contrasting OCR switch.
- **Copy as OCR** in the top toolbar recognizes screenshot text offline and copies
  it directly to the clipboard: Apple Vision on macOS, optional Tesseract on
  Omarchy, and a matching MCP action with source-region selection and read-back.

### Changed

- Grouped the account icon and **AI** button at the far right of the toolbar,
  separated from export controls.
- Ask Glance’s model dropdown opens only the model list above the prompt,
  keeping account settings separate.
- Ask Glance shows a spinner, elapsed time, and the current activity while running.
  The latest reply appears without an extra copy button.
- Ask Glance uses a single compact control row
  and no repeated plan/upload disclosures or Done/Undo banners. Account controls
  omit redundant explanatory text; image sharing behavior is documented in usage.

### Fixed

- Numbered callouts and text labels remain visible in previews and exports when
  the system annotation font is unavailable, using an embedded offline fallback.
- Ask Glance retains completed streamed tool calls and answers when the ChatGPT
  plan response finishes with an empty output array, avoiding false “no answer”
  failures. Failed, interrupted, or canceled requests still discard edits.

## [0.4.0] - 2026-10-06

### Changed

- Select mode supports dragging a selection box, Shift-click multi-selection and
  ⌘A/Ctrl+A to select all, with group dragging, styling, nudging, duplication and
  deletion.
- Linux animated backdrops and image entrance effects now use hardware Vulkan
  compute instead of CPU-only rendering, with cached GPU resources, adapter
  diagnostics, and CPU fallback when acceleration is unavailable.
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
