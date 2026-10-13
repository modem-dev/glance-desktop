# Glance on Omarchy

Glance supports Omarchy's Arch Linux / Hyprland desktop experimentally.
It requires a working Vulkan GPU driver and a Wayland compositor supporting `grim` capture.

## Install

Download the `.pkg.tar.zst` package and `arch-package.sha256` from the
[latest release](https://github.com/modem-dev/glance-desktop/releases/latest)
into the same directory, then verify and install:

```sh
sha256sum -c arch-package.sha256
sudo pacman -U ./glance-desktop-*.pkg.tar.zst
glance
```

Pacman installs the capture, clipboard, dialog, font, and FFmpeg dependencies.

## Capture from Hyprland

Use `glance --capture-area` to select a region or `glance --capture-screen` to
capture all displays. Escape cancels region selection. Each launch opens a new
editor; save or copy your work before closing it.

Add optional bindings to `~/.config/hypr/bindings.conf`, choosing keys that are free:

```ini
bindd = SUPER ALT, 2, Capture area in Glance, exec, glance --capture-area
bindd = SUPER ALT, 3, Capture displays in Glance, exec, glance --capture-screen
```

Run `hyprctl reload` to apply them. Open an existing image with
`glance --open /path/to/image.png` or the editor's Open button.

Use **Ctrl** in place of **⌘** for [editor shortcuts](usage.md): Ctrl+C copies,
Ctrl+Shift+C shares a temporary link, Ctrl+S saves PNG, and Ctrl+Z undoes.
Ctrl+Alt+2/3 captures while the editor is focused.

## Text extraction

Offline OCR uses optional Tesseract with English language data:

```sh
sudo pacman -S --needed tesseract tesseract-data-eng
```

Use **Copy as OCR** beside Copy in Glance's top toolbar to recognize text and
put it directly on the clipboard. Missing OCR dependencies leave the rest of the
editor usable and show an installation hint. See [Copy as OCR](usage.md#copy-as-ocr).

## Troubleshooting

- **Glance appears in the capture:** use the startup capture commands above.
  Hyprland may ignore the editor's request to minimize before toolbar capture.
- **Screen color picker fails:** install `hyprpicker` for **Pick from screen**.
- **Slow animated preview/export:** procedural backdrops and image entrance effects
  use hardware Vulkan compute, with CPU fallback if initialization or execution
  fails. Launch from a terminal and look for `Glance animation GPU:` to confirm
  the selected adapter, or a `using CPU fallback` warning. Software Vulkan
  adapters (such as llvmpipe) are not used for compute. Use a release build
  (`cargo run --release --locked`), especially when measuring fallback performance.
  GPU frames still require readback and upload to GPUI; compositing/export encoding
  are not fully GPU accelerated.
- **Window fails to open:** check that your GPU has a working Vulkan driver.

[Build from source](../BUILD.md#omarchy--arch-linux) ·
[Desktop acceptance checklist](../QA.md#omarchy)
