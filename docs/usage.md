# Using Glance

[Install](../README.md#install) · [Omarchy setup](linux.md) · [Tool options](../TOOL_OPTIONS.md)

Shortcuts below use macOS notation. On Omarchy, use **Ctrl** in place of **⌘**;
[Hyprland bindings](linux.md#capture-from-hyprland) provide global capture.

## Capture or open an image

- **⌘⌥2** captures an area; Escape cancels the selector.
- **⌘⌥3** captures the main display. Glance hides itself before capture.
- **⌘O** opens PNG/JPEG. You can also drop a file onto the canvas.
- **⌘V** loads a clipboard image when you are outside text editing.

The starter image is a practice canvas. Save or copy your work before capturing,
opening, pasting a replacement, or quitting; editable sessions last for the current launch.
On macOS, closing the window keeps global shortcuts active. Click the Dock icon
to reopen it; **⌘Q** quits.

For capture permission help, see the [macOS FAQ](../README.md#macos-faq).

## Annotate and select

| Tool | Key | How to use it |
| --- | --- | --- |
| Pen | P | Drag to draw |
| Arrow | A | Drag between endpoints; use the middle handle to bend it |
| Rectangle | R | Drag a box |
| Text | T | Click and type; Enter or clicking outside finishes, Escape cancels |
| Highlight | H | Drag over an area |
| Pixelate | B | Drag a region; block size is adjustable |
| Numbered callout | N | Click to place the next number |
| Spotlight | S | Drag a focus rectangle; the surrounding image dims |
| Magnifier | M | Drag from the source detail to the enlarged lens position |
| Crop | X | Drag to crop; undo restores the image |

Text labels are single-line; pasted line breaks become spaces. Text selection,
copy/paste, undo/redo, and native input methods work while editing.
Spotlights have corner handles; magnifiers have separate source and lens handles.
The magnifier enlarges the annotated foreground and keeps details bright within a spotlight.

New marks remain selected. Drag to move them, or click empty canvas to draw another.
Press **V** to select existing annotations. Click selects the topmost matching mark;
drag empty canvas to select marks whose bounds intersect the box.
**Shift-click** toggles one mark; **Shift-drag** adds to the selection.
**⌘A** selects all annotations, or text when editing a label or field.
Click empty canvas or press Escape to clear selection.

Drag any selected mark to move the group. **Delete**, **⌘D** (duplicate), and
**arrow keys** (nudge 1 pixel; Shift for 10) operate on the whole selection.
Color, width, and appearance edits apply to the group. Each group edit or drag
is one undo step; held nudge repeats are grouped. **⌘Z / ⌘⇧Z** undo/redo.

Hold **Shift** while drawing to snap arrows to 45° or make rectangles, highlights,
pixelation regions, and crops square. Shift-moving locks to the dominant axis.
Crop endpoints snap near image edges.

### Change tool options

The sidebar follows the selection or active tool, remembering each tool's defaults
for the session. Hover over **?** beside a tool name for help.

Click a numeric value to type an exact size or percentage. Enter or leaving the
field applies it; Escape cancels. Use a preset or custom color picker, enter
**#RGB / #RRGGBB**, or choose **Pick from screen**. Changing RGB preserves opacity.
Selected-annotation edits support undo. See the [tool reference](../TOOL_OPTIONS.md)
for line styles, pen cleanup, crop ratios, and other options.

## Resize, rotate, and navigate

Open **Image tools** to resize (50–400%) or rotate 90° clockwise. Smart upscale
resamples and sharpens the image. Resize redraws annotations at the new resolution;
crop and resize keep them editable. Rotation flattens them. Both support undo.
Resize is bounded to 16,000 pixels per side and 64 megapixels.
Backdrop padding stays in physical pixels.

Use **⌘1** to fit, **⌘0** for 100%, or **⌘+ / ⌘−** to zoom. Pinch zooms around
the pointer; two-finger scrolling pans. **⌘ + scroll** zooms; **Shift + wheel**
pans horizontally. Hold **Space** and drag, or right-drag, to pan.
Hold **Z** and click to zoom in at a spot; Shift-click zooms out.
Trackpad smart zoom toggles 100%/fit. Zoom ranges from 1–800%.

## Copy as OCR

Click **Copy as OCR** (the text-scanning icon beside Copy in the top toolbar), or
**Edit → Copy as OCR**. Glance recognizes text in the source screenshot locally
and puts it directly on your clipboard. Paste it into any text editor or app.
The toolbar shows **Reading text…**, then **Text copied!**; no result panel opens.
If no text is found or recognition fails, your existing clipboard is preserved.

Crop first to limit recognition to part of the image. OCR ignores annotations
and backdrops and does not change image pixels or image undo history.
Recognition preserves detected line breaks but does not reconstruct tables or
code indentation. Review the pasted text for recognition errors.

macOS uses the bundled Apple Vision helper. Omarchy uses optional `tesseract`
with its English language data; see [Linux setup](linux.md#text-extraction).
Local OCR needs no account, subscription, or upload. Work runs in the background,
with a 45-second timeout and a 64 KiB text limit; use a smaller crop if a limit is reached.

## Ask Glance

Click **Ask** in the toolbar, choose **Edit → Ask Glance…**, or press
**⌘K** (**Ctrl+K** on Linux). Type a request and press **Enter** or **Run**:

- “Pixelate the email addresses and highlight the error.”
- “Crop to the dialog and add numbered steps.”
- “Make this screenshot presentable with a rounded ocean backdrop.”
- “Explain this error and draft a bug report.”

Connect a ChatGPT account first. Ask Glance uses the selected account/model
independently of the **OCR source** setting; local OCR can remain selected.
The model control opens the account menu, where **Model** is a dropdown.
Running a request sends the current annotated image (as a downscaled preview),
annotation metadata, and your prompt to OpenAI using your ChatGPT plan.
Opening the prompt bar, typing, or signing in does not send the image.

The agent uses the same validated crop, resize, annotation, and backdrop actions
as the editor and MCP. It works on a private draft, inspects previews, and can
make several edits before finishing. A completed draft appears on the canvas as
**one undoable change**; annotations remain selectable and editable. Review
pixelated regions before sharing. **Stop** or **Esc** cancels a running request;
closing the bar also cancels it. Failure, interruption, cancellation, and
revision/account conflicts discard the draft and preserve your document.

The final reply appears below the prompt; **Copy answer** copies it. Ask Glance
can answer questions without changing the image. It does not export/upload
images, control other apps, or modify Glance's source code. Use the normal export
and copy controls after reviewing the result. Requests are bounded to eight
model rounds, 32 tool calls, five minutes, and 16 MP source images. Very large
contexts or framed previews may require a smaller crop or simpler request.

## Use your ChatGPT plan

Open the **ChatGPT account** icon in the top toolbar (or
**Glance → ChatGPT account…**) and choose **Continue with ChatGPT**. Finish signing in and granting permission in your
system browser. Eligible ChatGPT accounts can use their plan for image editing
and OCR without an API key. A first-use message explains plan usage; choose
**Got it** to continue.

In the account menu, select **ChatGPT** under **OCR source**, then choose an
available model from the **Model** dropdown. The same OCR
button now sends the source screenshot to OpenAI and copies the completed result
to the clipboard. Signing in alone does not upload images or select ChatGPT OCR.
Choose **On device** to use local recognition again. ChatGPT can help
preserve text layout and code indentation; review the pasted result for errors.

The menu shows saved accounts separately, including accounts with the same email.
Use the account dropdown to switch accounts or **Continue with ChatGPT** to add
another account/workspace. The circular-arrow icon reconnects the selected account;
the exit icon signs it out. Both have labels and hover hints. **Cancel sign-in**
stops a pending browser callback. Signing out clears the selected account’s local
credentials and attempts remote session revocation,
while preserving the account registration for later sign-in.

**Manage usage** opens ChatGPT usage settings. Requests count toward your ChatGPT
plan or available credits and may reach account/app limits. If permission or usage
is unavailable, Glance shows an error and preserves your clipboard. It does not
silently switch OCR providers. The integration currently uses OpenAI’s preview
subscription API; availability and models depend on the selected account.

Credentials are kept outside the repository in private, owner-only local files,
with atomic writes and serialized token refresh. MCP exposes account metadata,
model choices and OCR settings, never tokens. ChatGPT OCR sends at most a 16 MP,
20 MB source image, limits each network request to 90 seconds and returned text to 64 KiB,
and waits for complete inference before copying. Cropping limits the image sent.

## Frame the image

Open **Backdrop** and choose **Solid**, **Gradient**, or **Motion**.

- **Format** offers Auto, aspect ratios, and YouTube/Shorts/Pinterest presets.
  Fixed formats center the full screenshot within the expanded background.
- **Outside padding** sets the minimum backdrop margin.
- **Inside padding** extends the screenshot's edge pixels before rounding and shadow.
- **Image corners** and **Shadow** style the screenshot.

Choose a palette or click a swatch for custom colors. Solid uses one color;
Gradient and Motion use two. **Pick from screen** opens the macOS eyedropper;
Omarchy requires `hyprpicker`. Colors are opaque sRGB. Custom colors survive effect
changes; choosing a palette resets them. Slider/color gestures are one undo step.

**Done** closes the panel. **Enable backdrop** toggles framing while retaining its settings.

## Animate

For a moving background, choose **Backdrop → Motion** and an effect: Flow,
Nebula, Aurora, Contours, Painterly, Prism, Liquid, or Lava. **Randomize** creates
an undoable variation while retaining colors, framing, duration, and preview time.
Set a 2–15 second cycle (5 seconds by default).

For a foreground entrance, open **Animation** and choose **Diagonal reveal**,
**Spring pop**, or **3D settle**. Set duration, delay, and clip length.
The screenshot, annotations, corners, and shadow animate together, with any
backdrop style. Choose **Hold** to leave the image visible or **Exit** to return
to the empty backdrop before repeating.

**Replay** restarts; **Play/Pause** and **Preview time** let you inspect frames.
“Preparing preview…” appears during startup or seeking; playback waits for the
requested frame. Close the panel or click the canvas to return to editing.
Timing changes are undoable; replay and seeking leave history unchanged.

## Copy, save, or export

**⌘C** copies the composed image; **⌘S** saves PNG. Both retain full resolution.
With Animation open, PNG/copy capture the inspected frame; during normal editing
they capture the fully revealed image.

Use **Export → GIF… / MP4…** for an animated backdrop or image entrance.
Exports begin at time zero and preserve aspect ratio:

| Format | Output |
| --- | --- |
| PNG | Full resolution, including transparency |
| GIF | Infinite repeat, 20 fps, maximum edge 960 pixels |
| MP4 | H.264, 30 fps, maximum edge 1920 pixels |

GIF/MP4 use an ivory matte for transparent areas. Configure the video player to
loop an MP4. The export progress bar stays at the bottom of the window;
**Cancel export**, **Export → Cancel export**, or Escape stops it.

Save dialogs suggest timestamped names in Pictures (PNG/GIF) or Movies (MP4).
Use the matching `.png`, `.gif`, or `.mp4` extension. Glance preserves the
confirmed filename. If you omit an extension and the appended filename already
exists, choose the full filename in the dialog to confirm replacement.

### Share a link

**Copy (remote) · ⌘⇧C** uploads the composed PNG to [glance.sh](https://glance.sh)
and copies `Screenshot: <url>`. Wait for **Link copied!**, then paste it into a chat.
Failed uploads preserve the clipboard.

Links expire after about 30 minutes. Anyone with the link can retrieve the image.
Uploads are encrypted and require internet; the service allows up to 15 MB per
image and 30 uploads/hour per IP. Crop out sensitive content before sharing;
pixelation is a visual effect rather than reliable removal of information.
See the [security policy](../SECURITY.md) for data handling.

## macOS accessibility

Toolbar and inspector controls, numeric values, color hex fields, sliders, and
Format/Export menus are exposed to accessibility. Exact-value edits use the same
validation and undo behavior as mouse input. The canvas reports source dimensions
and annotation count; drawing and on-canvas text use ordinary input.
