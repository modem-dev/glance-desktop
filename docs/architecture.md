# Editor architecture


`main.rs` only launches the app and opens its window. The GPUI editor lives in
`src/editor/`: `mod.rs` constructs the entity, `state.rs` groups its state,
`actions.rs` defines typed, serializable application intent and `dispatch.rs`
is the common entry point. Toolbar buttons, native menus, shortcuts, gestures,
global hotkeys, file drops, and MCP adapters dispatch those values. `commands.rs`
implements edits and external work; `input.rs` interprets pointer/keyboard input.
`view.rs`, `canvas.rs`, and `panels/`
build and paint the interface. `text_input.rs` implements native text input;
`text.rs` owns the Unicode buffer and text history independently of the editor.

`src/color_picker.rs` is a reusable GPUI entity: it owns its swatch trigger,
anchored popup, HSV wheel/brightness state and native hex input in
`color_picker/input.rs`. Construct it with a label and the focus handle to
restore on dismissal, synchronize committed model colors through `set_value`,
and retain subscriptions to `ColorPickerEvent::{Changed, PickScreen}`.
Consumers dispatch their own typed actions; the component has no document or
backdrop dependency. Wheel and brightness gestures preview locally and emit one
committed RGB change on release. Its `ColorPicker` key context suppresses canvas
shortcuts, and clipboard/text undo stays within the hex field.

Annotation tools subscribe to the same picker, preserving their alpha channel
when dispatching `SetColor`. Source and screen eyedroppers use `SampleToolColor`
and `PickToolScreenColor`; screen results also validate the tool and selection.
`editor/panels/controls.rs` owns shared inspector fields, pairs and visual samples.
`panels/number.rs` owns bounded native numeric drafts and emits scoped values;
the editor translates them into existing typed size/color/appearance actions.
Drafts reset when their document revision or annotation/tool context changes.

Backdrop endpoints are optional opaque sRGB colors; absent endpoints preserve
legacy preset palettes. Both CPU and Metal renderers use them, and preview keys
include colors to reject frames from the old palette. The editor samples source
image pixels through the shared dispatcher. Explicit screen sampling uses
`platform/color_sampler.rs` (AppKit `NSColorSampler` or Omarchy `hyprpicker`) and
returns through the existing operation/revision checks.

One `Gesture` enum represents the active pointer interaction. `jobs.rs` owns
worker dispatch and completion: each external operation has an ID, and stale
results or video progress cannot affect a newer operation. Preview rendering
has its own revision checks and remains independent of external operations.
`feedback.rs` owns transient copy confirmations. `automation.rs` applies local
MCP requests on the UI thread, and `lens.rs` schedules magnifier previews.

`document/actions.rs` applies validated, undoable document edits for both the
editor and MCP snapshot workers. Heavy transforms remain on workers; prepared
MCP documents return through the dispatcher with an expected revision. Framing
changes and slider ticks also advance that revision, so stale work cannot
overwrite them. Native caret/IME handling and temporary pointer gestures stay
in input adapters; completed edits become document actions.

New triggers call `editor.dispatch(Action::SelectTool { tool: Tool::Arrow }, cx)`
instead of invoking input handlers or fabricating key events. The MCP
`dispatch_action` tool submits the same actions; `get_editor_state` reports
live state and background progress. See [action examples](../mcp/README.md#editor-actions).

MCP discovery has its own input schema in `src/mcp.rs`. Contract tests in
`src/mcp/contract_tests.rs` compare it with Serde's actual action inventory and
validate complete round-trip payloads for every exposed action. New commands
must ship their MCP path, read-back and docs together. Intentional exclusions
cover raw document indices and pointer slider gestures, which have semantic
MCP alternatives; prepared worker results cannot be deserialized.

The document, geometry, compositors, macOS integration, Glance protocol, and
video encoder remain separate modules. Interaction tests use GPUI's virtual
platform in `src/editor/tests.rs`, without controlling the user's desktop.

## Rendering and native integration

The app is Rust with GPUI; Metal shaders compile at runtime. Screenshot pixels
are immutable and shared across undo states. Geometry and annotations use
physical image pixels, including Retina captures. Live gestures paint GPU
overlays; compositing and export run on workers. Inside padding repeats the
nearest screenshot edge pixels before rounding and shadow. Worker-built editor
textures carry their padding amount with the revision, so stale textures cannot
change foreground geometry. Annotation coordinates remain in the original
capture, and animations transform the padded foreground as one image.

`src/animation/preview.rs` renders shader previews off the UI thread and adapts
quality to measured render cost. `src/shaders/motion.metal` and the CPU
implementations share the same periodic effects.
Nebula uses that worker for its nebula background, with crisp star points
painted above it in GPUI and composited above it by the export renderer.

`src/animation/entrance.rs` evaluates the independent image track: diagonal
alpha masking, spring scale, or a perspective projection. The annotated image
and shadow are cached once, and transformed sampling uses premultiplied alpha.
`src/animation/entrance/gpu.rs` and `src/shaders/entrance.metal` accelerate the
same masks and inverse projections on macOS preview/export workers. Each thread
keeps its pipeline and one source/output buffer; source ownership guards cache
identity. Hidden and settled poses stay exact; the CPU sampler remains the fallback.
`composition_preview.rs` uses a persistent worker with one running frame and
one latest pending request, rejects results after edits/seeks, and shares the
adaptive preview quality policy. Preview and PNG/MP4/GIF use the same compositor.
Prepared annotated pixels survive effect/timing and quality edits; only image or
annotation changes rerasterize them. Quality changes retain the last displayed
composition, and entering composition preview can retain the previous backdrop.
The canvas covers cold starts and seeks until a frame matching the document revision
and seek is painted. The playback clock holds during preparation and restarts
when the cover clears; shader warmup happens on the composition worker before
its initial frame is published. MCP exposes this as `playback.preparing`.
Entrances use absolute clip time; backdrop phase remains periodic. Video exports
start at time zero, and an optional exit restores the background at the loop end.
Normal editing uses the settled image pose, preserving annotation hit testing.

`src/platform.rs` owns macOS capture, clipboard, and file dialogs.
`native/video_encoder.swift` and `native/video_frame.swift` are small AVFoundation
helpers for encoding MP4 and reading video frames. They do not own the UI.

`ocr.rs` runs source-image text recognition on an editor worker, staging a PNG
in a private randomized temporary directory and bounding helper output and runtime.
macOS bundles `native/ocr.swift` (Apple Vision); Linux uses optional Tesseract.
The result belongs to editor state rather than the document. Operation and revision
checks reject stale results before writing the clipboard; `CopyOcr` shares
UI/MCP dispatch and state read-back. A nonempty result is copied directly without
opening a panel or changing image annotations and undo.

`chatgpt.rs` owns the opt-in ChatGPT subscription integration: private atomic
credential storage and cross-process locks, loopback OAuth with PKCE/state/nonce,
JWKS identity verification, refresh/revocation, account-specific model discovery,
and bounded Responses SSE parsing. Editor account jobs run outside the UI thread
and independently of document workers. Only public account snapshots enter UI or
MCP state; authorization URLs remain internal and are never logged. `CopyOcr`
selects local or ChatGPT recognition, and its operation/revision checks protect
the clipboard. Signing in alone never sends a screenshot.

`src/chatgpt/agent.rs` runs the native Ask Glance loop on a private render snapshot,
using a namespace of allowlisted local tools whose schemas, validation, and
`DocumentAction` execution are shared with MCP. Each completed SSE response
replays encrypted reasoning, calls, and tool outputs in a bounded client-owned
history; incomplete responses cannot execute tools. Preview rasterization,
encoding, inference, and draft edits stay on the worker. A successful draft is
folded into one history entry on a clone of the original document, then applied
through `ApplyPreparedDocument` with the starting revision. `src/editor/ask.rs`
checks operation/account/model identity and cancellation before dispatching it.
Cancel immediately detaches the active operation; late results and progress are
ignored. Its native prompt input has separate focus and text history.

`src/glance.rs` implements opt-in remote sharing. `src/mcp.rs` provides the stdio
MCP server, while `src/automation.rs` bridges it to an editor launched with
`--automation` over a local Unix socket.

[Performance notes](../PERFORMANCE.md) · [Test coverage](../QA.md) ·
[Original development history](../PLAN.md)
