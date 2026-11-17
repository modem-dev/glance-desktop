# Attn Lens app icon

`lens.png` is the production master: 1024 pixels square, a rounded macOS tile
with transparent margins. It is a close-up of attn's black cat with neon circuit
lines in its ears, on a calm violet and indigo sky, looking up at one large
glowing magenta tile, the agent waiting for you. Lens has marked the tile up:
capture corners around it, a hand-drawn circle and an arrow, with the cat
holding the stylus. attn's own app icon is the same close-up without the
markup, so the two read as one family in the Dock.

The artwork was generated with Codex image generation from attn's icon and
masked to a superellipse tile. The prompts, the candidates the user chose from
and the generator scripts are attached to attn's seed `s-yx4z1s`
(tie-attn-cat-logo-to-product).

`examples/icon.rs` resizes the master into all ten standard macOS iconset PNGs
with premultiplied filtering to preserve alpha edges. `scripts/bundle.sh`
packages them into a hash-named Attn Lens ICNS resource.

Earlier artwork and its generation prompts are preserved in `archive/`.
