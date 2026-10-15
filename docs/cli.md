# Terminal commands

Control Glance's running native editor from a shell. Commands use the same
validated actions as the UI and [MCP](../mcp/README.md), including editable
annotations, undo/redo, revision checks and export safety. No scripting runtime,
workflow service or additional dependencies are required.

## Start the editor

Automation is local and opt-in. Start an editor explicitly with:

```sh
glance --automation
# Equivalent explicit desktop command:
glance desktop --automation
```

Keep it running. On macOS, use the installed executable instead of `glance`:
`/Applications/Glance.app/Contents/MacOS/Glance`. For source builds, use
`target/debug/glance` after `cargo build --locked`.

Plain `glance` still opens the editor. Terminal commands never start or enable
the automation bridge themselves. `glance --mcp` remains the existing stdio MCP
companion; it is not replaced by a second MCP implementation.

## Edit and export

```sh
glance get-document
glance import-image --path /absolute/input.png
glance crop-image --x 20 --y 20 --width 300 --height 200
glance resize-image --scale 0.5 --smart=false
glance export-png --path /absolute/new-image.png
glance undo
```

Import replaces the current document and history. Other edits use native undo.
Exports refuse existing destinations, including symlinks. PNG/GIF/MP4 exports
use the same media helpers and validation as MCP. Work stays local unless you
explicitly invoke a sharing or cloud-AI action.

Successful commands write one JSON value to stdout. Errors write
`{"error":"message"}` to stderr and exit with status 1. Help is plain text;
`--schema` returns JSON. Large image reads return the same base64 PNG content
as MCP, not a screenshot upload.

## Discover commands and arguments

```sh
glance --help
glance resize-image --help
glance dispatch-action --schema
```

Every published MCP tool has a terminal command: replace underscores with
hyphens (`get_editor_state` → `get-editor-state`). Parameter names follow the
same rule (`expected_revision` → `--expected-revision`). Discovery and validation
come directly from the published MCP schema, so new shared actions appear here
without a second command inventory. See [the MCP reference](../mcp/README.md)
for each tool's behavior.

Flags accept `--name VALUE` or `--name=VALUE`. Boolean flags also accept a bare
`--name` for true. Omitted options retain their native defaults. Duplicate and
unknown flags are errors. Strings are literal; use the equals form for a string
beginning with `--`. Objects, arrays and nested null values use JSON:

```sh
glance import-image --clipboard
glance set-backdrop --enabled --backdrop '{"padding":40,"colors":null}'
glance dispatch-action --action '{"type":"select_tool","tool":"arrow"}'
glance dispatch-action --action '{"type":"copy_ocr","engine":"local"}'
```

`--cli` is an optional explicit prefix, for example `glance --cli get-document`.
There is no Code Mode or JavaScript execution command.

## Revisions and background work

Read `get-document` before editing objects. Its IDs are revision-scoped;
refresh them after an edit. Pass `--expected-revision N` to reject an edit if
another client or the UI changed the document since your read. Each command is
an independent operation, not a multi-command transaction.

Some `dispatch-action` calls start background work. An operation ID means work
was accepted, not completed. Poll `get-editor-state` for completion, status and
errors; do not treat a successful dispatch as a completed export or OCR result.
Use `dispatch-action --action '{"type":"cancel_export"}'` to cancel an animation
export. OCR can use a saved cloud engine; select `"engine":"local"` explicitly
when offline recognition is required.
