# README media

`glance-example.png` is a real Glance export using the built-in practice canvas,
a spotlight, a magnifier, and the Lava backdrop at phase 0.2. It contains no
personal screen content.

To regenerate it after building the native helpers:

```sh
cargo test --release --locked focus_and_loop_demo_qa -- --ignored --nocapture
cp target/focus-qa/focus-demo.png docs/assets/glance-example.png
```

That opt-in test also produces a GIF and MP4 in `target/focus-qa/`; only the PNG
is tracked here. The image uses the same compositor as the app's exports.

## Album animation

`glance-albums.gif` is a six-second, infinitely looping preview of the light and
dark MY9ALBUMS exports supplied by the project owner. Glance's native compositor
renders the violet Lava backdrop, 18 px of edge padding, 64 px of outer padding,
rounded corners, and shadow. The galleries remain still while the backdrop
moves, switching between light and dark without entrance or exit effects.
FFmpeg optimizes the GIF to 534×600 at 12.5 fps with a shared palette; the
result is approximately 2.4 MB.

Source exports are 1800×2100 PNGs; the originals are kept outside Git:

| Source | SHA-256 |
| --- | --- |
| `my9albums.png` (light) | `a4d1e4d848f813025d29a403eee9ca4672709a1f437f7d4cbe8ae6616fed1365` |
| `my9albums (1).png` (dark) | `cd0dd9379c417d9cb3c82396032cc16d7b5bf8471941e0eddb91e22ab1d67ff4` |

Album artwork belongs to its respective rights holders and is shown as part of
the supplied gallery exports. The repository's MIT license covers Glance's
software, not the album artwork.

Regenerate with Rust/Cargo and FFmpeg; the generator is opt-in and never captures
screens or uploads media:

```sh
mkdir -p target
export GLANCE_README_OUTPUT="$(mktemp -d "$PWD/target/readme-albums.XXXXXX")"
export GLANCE_ALBUM_LIGHT="/path/to/my9albums.png"
export GLANCE_ALBUM_DARK="/path/to/my9albums (1).png"
cargo test --release --locked render_readme_album_gif -- --ignored --nocapture
ffmpeg -hide_banner -loglevel warning -n -protocol_whitelist file \
  -i "$GLANCE_README_OUTPUT/albums-native.gif" \
  -filter_complex '[0:v]fps=12.5,scale=534:600:flags=lanczos,split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=none:diff_mode=rectangle' \
  -loop 0 "$GLANCE_README_OUTPUT/albums-compact.gif"
cp "$GLANCE_README_OUTPUT/albums-compact.gif" docs/assets/glance-albums.gif
```

`glance-ocr.png` shows the Copy as OCR toolbar button in an isolated macOS debug
build using Glance's built-in synthetic practice image. It contains no user capture.
