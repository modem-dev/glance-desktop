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
glance desktop
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

## Troubleshooting

- **Glance appears in the capture:** use the startup capture commands above.
  Hyprland may ignore the editor's request to minimize before toolbar capture.
- **Screen color picker fails:** install `hyprpicker` for **Pick from screen**.
- **Slow animated preview/export:** Linux currently renders motion on the CPU.
- **Window fails to open:** check that your GPU has a working Vulkan driver.

[Build from source](../BUILD.md#omarchy--arch-linux) ·
[Desktop acceptance checklist](../QA.md#omarchy)
