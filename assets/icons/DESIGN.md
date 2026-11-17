# Attn Lens app icon

`lens.png` is the production master: 1024 pixels square, a rounded macOS tile
with transparent margins. It is attn's black cat with neon circuit lines in its
ears, looking up at a quiet wall of agent tiles on a calm violet and indigo sky.
One tile glows magenta, the agent waiting for you, and Lens has marked it up:
capture corners around it, a hand-drawn circle and an arrow, with the cat
holding the stylus. attn's own app icon is the same scene without the markup,
so the two read as one family in the Dock.

The artwork was generated with Codex image generation from attn's icon and
masked to a superellipse tile. The prompts, the candidates the user chose from
and the generator scripts are attached to attn's seed `s-yx4z1s`
(tie-attn-cat-logo-to-product).

`examples/icon.rs` resizes the master into all ten standard macOS iconset PNGs
with premultiplied filtering to preserve alpha edges. `scripts/bundle.sh`
packages them into a hash-named Attn Lens ICNS resource.

Earlier artwork and its generation prompts are preserved in `archive/`.
