# Glance local MCP companion

An MCP client can read, annotate, and export the image in Glance's native window.
Start the editor with `--automation`, then connect its stdio companion with `--mcp`.

## Setup

[Install Glance](../README.md#install), quit any running instance, and launch the
installed macOS bundle with automation enabled:

```sh
/Applications/Glance.app/Contents/MacOS/Glance --automation
```

Configure your local MCP client:

```json
{
  "mcpServers": {
    "glance": {
      "command": "/Applications/Glance.app/Contents/MacOS/Glance",
      "args": ["--mcp"]
    }
  }
}
```

On Omarchy, launch `glance --automation` and use `/usr/bin/glance` as the MCP command
with `args: ["--mcp"]`. Keep the editor running and use the same build for both processes.

For source builds, follow [BUILD.md](../BUILD.md), launch the built executable
with `--automation`, and configure the absolute path to `scripts/mcp.sh` as your
client command with no arguments. Restart both processes after rebuilding.

### ChatGPT

Use [Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels)
with a local stdio profile whose MCP command is
`/Applications/Glance.app/Contents/MacOS/Glance --mcp` (or `/usr/bin/glance --mcp`
on Omarchy). Follow the official setup instructions and keep the tunnel client running.
Then follow [Connect and test](https://developers.openai.com/plugins/deploy/connect-chatgpt)
to select the tunnel in ChatGPT. Access depends on organization and workspace policy.

A remote client receives the previews returned by Glance through its transport.
Supply image bytes as base64 or stage files on the computer running Glance.

## First edit

1. Call `import_image` with exactly one of `path`, `base64`, or `clipboard: true`.
   This replaces the current image and its undo history.
2. Call `get_document` for dimensions, revision, and annotation IDs.
3. Call `add_annotation` with an arrow:

   ```json
   {"mark":{"tool":"arrow","points":[[100,100],[400,250]],"curve":[260,60],"color":[255,56,100,255],"width":5,"text":""}}
   ```

4. Call `read_image` to inspect, then `export_png` to save to a new local file.

Coordinates are physical source-image pixels. Use IDs from the latest
`get_document` when updating, moving, or deleting marks. IDs expire after each
edit; pass `expected_revision` with mutations to reject stale requests.
PNG previews are inline content; exported media paths refer to the computer running Glance.

## Tools

| Tool | Purpose |
| --- | --- |
| `open_editor` | Bring the native window forward |
| `get_editor_state` | Tool options, selection, zoom, panels, playback, status, and operation progress |
| `get_document` | Dimensions, revision, backdrop, image animation, marks, and IDs |
| `dispatch_action` | Run a typed editor command; see examples below |
| `import_image` | Load one local `path`, image `base64`, or `clipboard: true` |
| `add_annotation`, `update_annotation`, `move_annotation`, `delete_annotation` | Edit annotation objects |
| `crop_image`, `resize_image`, `set_backdrop`, `undo`, `redo` | Edit the document |
| `read_image` | Inline PNG preview, optional animation `phase` 0–1; maximum edge defaults to 1600, without upscaling |
| `export_png` | Full-resolution PNG |
| `export_mp4` | H.264, 30 fps, 2–15 seconds, maximum edge 1920 |
| `export_gif` | Infinite-repeat GIF, 20 fps, maximum edge 960 |
| `read_video_frame` | Inline PNG of a local MP4 at `seconds` |

MP4/GIF require a motion backdrop or image entrance and begin at time zero.
Exports return a MIME type and local path; animated exports also return duration
and fps. Inspect a video with `read_video_frame` using its path and timestamp.

## Editor actions

Call `dispatch_action` with an `action` object. `type` chooses the command:

```json
{"action":{"type":"select_tool","tool":"arrow"}}
{"action":{"type":"fit"}}
{"action":{"type":"pan_by","delta":[40,0]}}
{"action":{"type":"copy_image"}}
{"action":{"type":"resize","scale":2,"smart":true},"expected_revision":7}
{"action":{"type":"set_backdrop_format","format":"shorts"}}
{"action":{"type":"toggle_backdrop_enabled"}}
{"action":{"type":"export_animation","format":"gif"}}
```

Actions share UI validation and undo. `copy`, `cut`, `paste`, `undo`, `redo`, and
`delete` operate on inline text during editing. `copy_image`, `copy_remote`, and
`paste_image` commit that text and act on the image.

A receipt such as `{"revision":7,"operation_id":12}` accepts background work.
Poll `get_editor_state` for `busy`, operation ID/kind/progress, and status to check
completion. `cancel_export` cancels an active animation export; progress matches
the native export bar. State inspection and cancellation remain available while busy.
Capture, open, save, and export actions use native permissions/dialogs;
path-based export tools save directly to files.

### Select annotations

Use `select_all`, `select_annotations` with revision-scoped `ids` from
`get_document` (`[]` clears selection), or `select_region` with
`rectangle: [x, y, width, height]` and optional `additive: true`.
The region selects marks whose bounds intersect it. `get_editor_state` returns
`selected_indices`, revision-scoped `selected_ids`, and the primary inspector index
as `selected`.

`select_all` selects text during an inline edit; finish editing before selecting
explicit annotation IDs. `nudge_selection`, `delete`, `duplicate_selection`, color,
width, and appearance changes apply atomically to the group with one undo step.
Supply `expected_revision` to reject stale requests.

### Annotation options

Annotation tools accept pen, arrow, box, text, highlight, pixelate, counter,
spotlight, and magnifier. Text uses one point; font size is `width × 7` pixels.
Spotlight uses opposite rectangle corners. Magnifier uses source and lens centers;
radius is `width × 12` pixels (18–300), and `text` sets zoom (1.5–4, default 2).
Replace its points to move source and lens independently.

`get_editor_state.tool_options` reports the selection or active tool's settings:

```json
{"type":"set_appearance","style":{"dash":"dashed","start":"none","end":"arrow"}}
{"type":"set_appearance","style":{"fill":"filled","radius":12}}
{"type":"set_appearance","style":{"cleanup":"adaptive"}}
{"type":"add_line_point"}
{"type":"straighten_line"}
{"type":"set_magnifier_zoom","zoom":3}
{"type":"set_counter_number","number":5}
{"type":"set_crop_ratio","ratio":1.7777778}
{"type":"set_color","color":[18,171,239,128]}
{"type":"sample_tool_color","position":[20,40]}
{"type":"pick_tool_screen_color"}
```

These objects belong inside `dispatch_action.action`. Appearance replaces the
style; omitted fields take defaults. Changes edit the selection and remember the
tool's next-mark settings. `add_line_point` and `straighten_line` require a selected
line. Marks also accept an optional `style` object. Lines allow up to 32 points;
`curve` controls a two-point line. Opacity is `set_color`'s alpha channel.

`sample_tool_color` samples an unannotated source pixel, preserves opacity, and
supports undo on selected marks. Invalid positions or tools without colors are rejected.
`pick_tool_screen_color` opens the native sampler and returns an operation ID;
results require the same operation, revision, and annotation/tool target.
`get_editor_state.sampling_tool_color` reports internal canvas sampling.
Use `sample_tool_color` for coordinate-based automation.

Annotation tools accept positive widths and corner radii up to 32768 pixels to
keep resized marks editable. `set_stroke_width` uses the sidebar's 0.5–64 range.
Resizes reject geometry outside document limits before changing history.

### Backdrops

```json
{"action":{"type":"set_backdrop","backdrop":{"motion":"lava","preset":0,"padding":100,"inside_padding":24,"seconds":5,"inner_radius":18,"shadow":24}}}
```

`padding` sets the backdrop margin; `inside_padding` repeats source edge pixels
before rounding and shadow. Both accept 0–512 physical pixels. Annotation
coordinates remain relative to the original image. `set_backdrop` is also a
document tool with a `backdrop` parameter.

For custom colors, supply `colors: [[r,g,b],[r,g,b]]` (opaque sRGB, 0–255), or
`colors: null` to use `preset`. `get_document` returns the endpoints. Solid uses
the first color; gradient and motion use both. Shared actions include:

- `set_backdrop_color`: `stop` (0 or 1) and `rgb` (three channels). Preserves the
  other endpoint, initially from the preset; supports undo.
- `sample_backdrop_color`: `stop` and `position: [x,y]` in unannotated source pixels.
- `pick_backdrop_screen_color`: `stop`. Opens the macOS sampler or Omarchy's
  `hyprpicker` and returns an operation ID. Cancellation preserves the document;
  delayed results require the same operation and revision.

Use `expected_revision`. Invalid samples leave the document unchanged.
`set_backdrop_preset` clears custom colors; changing fill or motion preserves them.

Use `"motion":"nebula"` with `set_backdrop` or `select_motion`; the legacy
`"stars"` alias is accepted and reads back as `"nebula"`.
`randomize_motion` requires a moving backdrop, supports undo, and preserves
colors, framing, duration, and paused time. `get_document` returns `backdrop.seed`:

```json
{"action":{"type":"randomize_motion","seed":42},"expected_revision":3}
```

Seeds range from 0–4294967295; 0 restores the original composition.
Omitted/null seeds choose a new nonzero value. `set_backdrop` also accepts `seed`.
Preview and PNG/GIF/MP4 use the same looping variation.

### Image entrances

Use `toggle_animation_panel`, `select_entrance` (`effect`: `none`, `diagonal`,
`pop`, or `tilt`), `set_image_animation` (`animation`: `effect`, `duration_ms`,
`delay_ms`, `seconds`, `exit`), `set_animation_control`, `seek_animation`
(`seconds`), and `replay_animation` through `dispatch_action`.

`get_document.image_animation` returns the track; `get_editor_state` returns the
panel, playback time, and `playback.preparing`. Playback holds during startup or
seeking until a matching frame is ready. Entrances work over moving, still, or
absent backdrops. MP4/GIF begin at time zero.

## Behavior and limits

Document edits preserve native undo and apply only if the revision is unchanged
and no manual gesture/text edit is in progress. New marks are selected for manual
editing. Requests are serialized; synchronous media exports can delay subsequent
MCP calls while the native window remains responsive. `dispatch_action` accepts
background operations immediately.

Local imports and video reads require absolute paths to regular files.
Image imports are bounded to 16 MiB encoded / 32 megapixels; actual reads enforce
the size limit even for growing files. Media helpers reject network inputs.
Exports default to the private `automation/exports` directory. Explicit paths
must be absolute and new: existing files are rejected.

The macOS socket is `~/Library/Caches/sh.glance.desktop/automation/editor.sock`;
Linux uses `$XDG_CACHE_HOME/glance/automation` (or `~/.cache/glance/automation`).
The directory is mode 0700 and socket mode 0600. Processes running as your user
can access the bridge. Enable it for trusted clients; see [SECURITY.md](../SECURITY.md).

## Verification

`cargo test --locked` covers discovery/schema contracts, bridge edits/read-back,
undo, stale revisions/IDs, and output-file protection on GPUI's virtual platform.
See [QA.md](../QA.md) for native media and desktop checks, and
[architecture](../docs/architecture.md) for dispatcher and schema conventions.

## Incurs and Code Mode

The shell command provides `glance --mcp` for the complete Incurs editor
interface and `Glance --codemode-mcp` for the five Code Mode lifecycle tools.
Start `glance code serve` for shared execution state. The app bundle's original
`Glance --mcp` remains supported; `Glance --cli --mcp` selects Incurs explicitly. See
[CLI, MCP, and Code Mode](../docs/interfaces.md) for setup and command mappings.
