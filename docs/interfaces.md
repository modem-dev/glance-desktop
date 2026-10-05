# CLI, MCP, and Code Mode

For terminal users and coding agents controlling the native Glance editor.
This fork uses published Rust Incurs 0.10.3 and Code Mode 0.8.0. No JavaScript
build tools are required.

Build with `cargo build --locked`. The examples use `target/debug/glance`;
a packaged app's executable also accepts direct subcommands.

The shell command defaults to the CLI: plain `glance` prints help, and
`glance get-document` runs a command. `glance desktop` opens the native editor.
Launching the macOS app bundle still opens its window. The legacy `--cli` prefix
and desktop startup flags remain supported.

## Start the editor

```sh
target/debug/glance desktop --automation
```

The editor owns the document, undo history, and background export operations.
Automation stays local and opt-in. All coordinates are source image pixels,
excluding backdrop padding. Import replaces the current document.

## CLI and regular MCP

```sh
target/debug/glance --help
target/debug/glance get-document
target/debug/glance dispatch-action --action '{"type":"fit"}'
target/debug/glance resize-image --scale 0.5 --smart=false
target/debug/glance export-png --path /absolute/new-image.png
target/debug/glance --mcp
```

CLI command names and flags use hyphens. MCP names and parameters retain the
existing underscore spelling. Use JSON text for objects, arrays, and nullable
values. Omitted settings retain the editor command's defaults. Boolean flags
accept `--smart=false` as well as `--smart`.

`--mcp` serves the Incurs MCP interface, including structured editor
tools and Code Mode lifecycle commands. Image reads include model-visible PNG
content. `--native-mcp` serves the original editor-only MCP interface. The macOS
app executable's `--mcp` retains that original interface; use `--cli --mcp` on
the app executable to explicitly select Incurs.

| Editor MCP tool | CLI command |
| --- | --- |
| get_editor_state | get-editor-state |
| dispatch_action | dispatch-action |
| open_editor | open-editor |
| get_document | get-document |
| import_image | import-image |
| add_annotation | add-annotation |
| update_annotation | update-annotation |
| move_annotation | move-annotation |
| delete_annotation | delete-annotation |
| crop_image | crop-image |
| resize_image | resize-image |
| set_backdrop | set-backdrop |
| undo | undo |
| redo | redo |
| read_image | read-image |
| export_png | export-png |
| export_mp4 | export-mp4 |
| export_gif | export-gif |
| read_video_frame | read-video-frame |

`dispatch_action` exposes all published application actions, including tool
settings, framing, animation, playback, selection, capture, and export controls.
`dispatch-action --schema` describes the CLI flags. The complete nested
action inventory is published by MCP discovery and
`code search --query dispatch_action` after starting the Code Mode service.
See [the editor MCP reference](../mcp/README.md) for parameters and semantics.

Read `get_document` before object edits and pass `expected_revision` for
optimistic concurrency. Object IDs are revision-scoped. Refresh them after
every edit. Accepted background work returns an operation ID; use
`get_editor_state` to check completion. Code Mode cancellation does not undo
completed edits or cancel an accepted media export; use native undo or
`dispatch_action({action:{type:"cancel_export"}})` for those operations.

## Shared Code Mode service

Start this explicit local service in a second terminal:

```sh
target/debug/glance code serve
```

It runs in the foreground until stopped. A private Unix socket lets independent
CLI and MCP clients control the same executions. A lifetime lock refuses a
second service. SQLite stores execution history and owned oversized results
under the private Glance automation cache directory. `GLANCE_CODE_DIR` selects
an absolute private service directory for isolated sessions.

```sh
target/debug/glance code search --query annotation
target/debug/glance code execute --code 'const doc = await glance.get_document({}); return {revision: doc.revision, objects: doc.objects.length};'
target/debug/glance code execution --id EXECUTION_ID
target/debug/glance code cancel --id EXECUTION_ID
target/debug/glance --codemode-mcp
```

`execute` returns an execution ID and running state. Read `execution` until
it reports a terminal status. An execution that failed has an error in its
record; accepting the program does not establish successful execution.
Programs support async/await, loops, conditional logic, and `Promise.all`.
Search returns TypeScript declarations for the complete `glance` namespace.
The sandbox has no arbitrary filesystem, process, or network API. Uninterrupted
JavaScript has a one-second wall-clock budget, renewed around editor tool calls.
Cancellation updates the shared runtime immediately, and CPU-bound JavaScript
stops within that evaluation budget; it does not wait behind the actor loop.

| Code Mode MCP tool | CLI equivalent |
| --- | --- |
| codemode_search | code search --query TEXT |
| codemode_execute | code execute --code JAVASCRIPT |
| codemode_execution | code execution --id ID [--artifact-id ID] |
| codemode_decide | code decide --id ID --seq N --decision approve/reject |
| codemode_cancel | code cancel --id ID |

`--codemode-mcp` exposes exactly these five lifecycle tools. Configure an MCP
client to launch the Glance executable with that argument after starting the
service. Search, execution lookup, artifact ownership, deterministic replay,
approval decisions, bounded JavaScript, and cancellation use the published
Incurs runtime. Local reads proceed automatically; document edits and exports
pause for an approval decision. Recorded reads preserve revision-scoped
arguments across approval replay, so completed edits run once. Use execution
read-back to find the pending log entry's sequence, then approve or reject it
with `decide`. An arbitrary decision sequence is rejected. History survives service restarts; interrupted
running programs are failed on restart and are never silently replayed.

## Coverage and limits

The catalog registers every existing editor tool with its exact published
schema and behavioral annotations. Tests use an independent payload inventory,
compare the action inventory against Serde, exercise the native GPUI dispatcher
with state read-back, undo/redo and stale revisions, and run Code Mode lifecycle
and artifact-persistence checks. These are API and native virtual-platform
checks; they do not establish physical screen-capture permission or production
remote-sharing availability. After building, run
`python3 scripts/check-interface.py target/debug/glance` to verify the actual CLI,
both MCP transports, cross-client state, and cancellation. This check uses a
private temporary service directory and never applies an edit to a native editor.
