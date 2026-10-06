# Glance QA

Use synthetic images for acceptance checks. Record the build/commit, OS, hardware,
checks performed, and failures with each result. Prior results and benchmarks are
in the [October 2026 QA record](docs/qa-history.md); they describe those builds.

## Automated checks

Run the [contributor checks](CONTRIBUTING.md#make-a-pull-request).
The regular suite covers the document model, rendering, MCP contracts/bridge,
and GPUI's virtual platform. Check physical input and display behavior separately.

### Opt-in checks

Read each ignored test before running it. Build the [native helpers](BUILD.md)
for video checks. These tests use synthetic content; media-generating checks write
under `target/`. Run only the checks relevant to the change:

| Check | Command |
| --- | --- |
| Native MP4 encode/frame decode | `cargo test --locked native_mcp_video_roundtrip -- --ignored --nocapture` |
| Local OCR on synthetic text, source crop and blank image | `cargo test --locked native_ocr_recognizes_synthetic_text_and_regions -- --ignored --nocapture` |
| Image entrance export samples | `cargo test --release --locked image_entrance_export_qa -- --ignored --nocapture` |
| Focus effects and loop samples | `cargo test --release --locked focus_and_loop_demo_qa -- --ignored --nocapture` |
| Native motion export samples | `cargo test --release --locked native_motion_export_qa -- --ignored --nocapture` |
| Drawing preparation benchmark | `cargo test --release --locked drawing_preparation_benchmark -- --ignored --nocapture` |
| Foreground sampling benchmark | `cargo test --locked foreground_sampling_benchmark -- --ignored --nocapture` |

Benchmarks measure their documented worker/preparation boundaries; see
[PERFORMANCE.md](PERFORMANCE.md). Test remote sharing with local fixtures.
The separately ignored live-upload test sends an image to production; run it only
with explicit authorization for that verification.

## macOS desktop acceptance

- Install/open the release binary, follow the opening FAQ, grant Screen Recording,
  and capture an area/main display. Check selector cancellation and capture after relaunch.
- Test toolbar/menu/global shortcuts, clipboard import/copy, file drop, open/save
  cancellation, overwrite confirmation, and extension handling.
- Draw each annotation. Select, move, restyle, duplicate, delete, and undo/redo.
  Check curved-arrow handles, spotlight corners, and magnifier source/lens handles.
- Box-select several marks; test Shift-click/drag, group edits and one-step undo.
  Verify Select All works in the canvas, inline text, numeric fields, and hex fields.
- Test text selection, Unicode/IME, copy/paste, caret placement, and tool shortcuts
  while editing. Test zoom/pan, pinch, smart zoom, and wheel momentum.
- At minimum window size, inspect toolbar/sidebar fit, numeric labels/values,
  scrolling, tool help, and exact-value edits. Check backdrop and annotation color
  pickers, screen sampling/cancellation, opacity, and undo.
- Test framing, crop, resize, and rotation; inspect PNG/clipboard output at full resolution.
- Click **Copy as OCR** on synthetic multiline text and paste into a text editor.
  Confirm the toolbar shows reading/copied feedback, no result panel opens, and
  image history is untouched. Empty images and failures preserve the clipboard;
  stale results cannot overwrite it. Confirm recognition stays offline.
- Check accessible toolbar/inspector/menu discovery, exact values, and VoiceOver navigation.

## ChatGPT account controls

- Open ChatGPT from the toolbar; check account controls remain accessible and fit
  a small window. Check contrast, account/model dropdowns, arrow/Enter/Escape
  navigation, outside-click dismissal and icon hover/accessibility labels. Models
  appear only for ChatGPT OCR. Confirm local OCR works without an account or network.
- With explicit authorization for the test account, use Continue with ChatGPT in
  the system browser. Check the first-use modal, account/model display, restart
  persistence, reconnect, account/workspace switching, cancellation and sign-out.
- Select ChatGPT OCR and use only a synthetic image. Check the clipboard after
  completed inference and compare offline recognition. Verify missing permission,
  expired sign-in, model errors, usage limits, incomplete/interrupted streams and
  sign-out leave existing clipboard text intact.
- Confirm MCP account state never includes credentials, and screenshots/support
  logs contain no tokens or authorization URLs.

## Animation and export

- Preview all eight motions, Randomize, custom colors, and duration changes.
- Check each entrance over still/moving/absent backdrops, timing, Hold/Exit,
  replay, pause, seeking, and return to annotation editing.
- With a large image, replay/seek while preparing. The cover must remain until
  the requested frame is ready; paused playback stays paused. New edits/replays
  reject stale frames. Ordinary frame/quality changes should stay continuous.
- Compare inspected-frame PNG/copy with time-zero GIF/MP4 exports. Check dimensions,
  duration, matte, stable foreground, and loop seams visually.
- Close/scroll the inspector during export: progress stays visible. Cancel and
  confirm existing files survive and temporary files are cleaned up.

## MCP

Start an opted-in editor and connect a trusted client using [MCP setup](mcp/README.md).
Import a synthetic image; annotate, select a group, restyle, undo, read back, and
export. Check revision conflicts and stale IDs, operation progress/cancellation,
and rejection of existing output paths. Verify a ChatGPT tunnel separately when
that connection is part of the change.

## Omarchy

On a real Hyprland desktop, verify Vulkan startup, scaled/multi-display capture,
area cancellation, optional global bindings, window close, clipboard persistence,
Ctrl shortcuts, text/IME, dialogs/overwrites, and `hyprpicker` selection/cancellation
or its missing-command error. Inspect PNG/GIF/MP4 output and animation performance.
Use the [Omarchy guide](docs/linux.md) for installation and bindings.
