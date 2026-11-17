# Pachiri app icon

`pachiri-teal-p.png` is the active generated master with transparent margins.
It combines a thick, smooth ivory p with thinner capture brackets on a teal tile.
The p has rounded terminals and no arrowhead. `pachiri.png` and
`pachiri-monogram.png` retain the earlier pencil designs. The
bundle's `examples/icon.rs` creates all ten standard macOS iconset PNGs (16–1024
physical pixels), filtering premultiplied colors to preserve clean alpha edges.
`scripts/bundle.sh` packages them into `Pachiri.icns` for Dock, Finder, Spotlight
and the app switcher.

The bundled ICNS filename includes the generated artwork's hash, so a changed
design gets a fresh resource name instead of reusing macOS's previous icon cache.

Generated using the built-in image_gen tool on 2026-10-03.

## Generation prompt

### Final teal P (built-in image_gen edit)

Reference: `explorations/07-arrow-bold-p.png`.

Use case: precise-object-edit / logo-brand. Make ONE extremely minimal edit to the supplied selected Pachiri macOS app icon: REMOVE the arrowhead at the bottom of the central handwritten p. Replace that arrowhead with a simple smoothly rounded end-cap of the same thickness as the p stroke. Preserve the exact centerline and ending position of the descending p stem; do not bend it into a new direction, extend it, shorten it appreciably or make it look like a pencil. The result is a single thick flowing ivory p with an open loop and round terminals, surrounded by the existing four thinner ivory capture brackets. EVERYTHING ELSE must remain unchanged: exact thick p stroke weight, loop shape and angle, thinner bracket thickness, bracket positions and shape, teal tile color and shading, macOS rounded-square silhouette, image dimensions, transparent margins, ivory material, subtle bevel and shadows. No arrowhead, no tiny arrow remnant, no serif or extra ornament, no pencil, no new details. Output the actual finished icon with genuine alpha transparency outside the rounded-square tile.

### Monogram revision (built-in image_gen edit)

Use case: precise-object-edit / logo-brand. Edit the supplied Pachiri macOS app icon. Preserve the coral rounded-square tile, its exact shape, size, subtle enamel shading, transparent outside margins, ivory material, and four capture-frame corner brackets. Redesign ONLY the central pencil emblem into a classy integrated capital P monogram: the pencil itself forms the long stem of the P, with a pointed writing nib at its lower end; a clean rounded bowl joins the upper half of that pencil stem, enclosing generous coral negative space. The whole monogram can tilt slightly clockwise like a poised pencil, but the P must read unmistakably as P (not R or D) at small Dock sizes. Think bespoke geometric lettering rather than an ordinary typed letter: one simple cohesive ivory pencil-P symbol, modest tactile bevel and shadow matching the capture brackets. Keep it balanced and centered within the brackets with clear separation and breathing room. No extra lettering, no wordmark, no additional standalone P beside a pencil, no ornate flourishes, no new colors, no mockup, no checkerboard. Deliver the finished square icon with actual alpha transparency outside the tile. Prioritize clarity and quiet typographic elegance.

### Original design

Use case: logo-brand. Asset type: production macOS application icon for Pachiri, a fast screenshot capture and annotation app. Generate one single finished square icon, straight-on, centered, on a genuinely transparent canvas. Design: a warm coral-red macOS rounded-square tile with generous continuous rounded corners, subtle crafted depth, a softly luminous upper edge and very restrained shadow. Within it, a bold ivory capture-frame symbol formed by four thick rounded viewfinder corner brackets, combined with one beautiful simple diagonal ivory annotation pencil, integrated into the frame as a single memorable emblem. Keep the silhouette bold, balanced and instantly legible at 32px in the Dock and app switcher. Premium friendly native Mac utility aesthetic; smooth enamel-like surface, mostly flat with modest tactile shading, clean geometry and ample internal breathing room. Tile occupies about 82% of square canvas width and height, standard macOS icon optical sizing, centered with transparent margins on all four sides. Coral palette echoes Pachiri's existing warm red/orange accent. No text, no letters, no watermark, no mockup, no device, no scene, no extra decorative sparkles, no multiple variants. Output the actual icon artwork with real alpha transparency outside the rounded tile, not a checkerboard painted into the image.

## Attn Lens cat with a magnifying glass

`lens-cat-magnifier.png` was Attn Lens's icon before the quiet redesign: attn's
cat on a streaky magenta, violet and cyan aurora, holding a magnifying glass to
one eye. Its prompts and the rounds it was chosen from are attached to attn's
seed `s-7eyt9h` (make-new-attn-logo).
