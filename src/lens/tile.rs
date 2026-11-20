//! The corner tile a closed capture shrinks to (D13 on the plan). A sent capture's
//! tile leaves after a few seconds unless hovered; an unsent one stays. A click
//! reopens the capture.
use super::{Lens, Summary, session::DisplayFrame};
use rune::{Color, Cx, El, col, el, text};
use std::sync::Arc;

const SIZE: (f32, f32) = (176., 128.);
const MARGIN: f64 = 16.;

/// The tile's window, at the display's bottom-right corner.
pub fn placement(display: DisplayFrame) -> [f32; 4] {
    [
        (display.frame[2] - f64::from(SIZE.0) - MARGIN) as f32,
        (display.frame[3] - f64::from(SIZE.1) - MARGIN) as f32,
        SIZE.0,
        SIZE.1,
    ]
}

pub struct Tile {
    pub lens: Arc<Lens>,
}

pub fn tile_view(s: &Tile, _: &mut Cx<Tile>) -> El {
    let k = crate::kit::hud();
    s.lens.read(|summary: &Summary| {
        let Some(tile) = &summary.tile else {
            return el().key("tile-empty");
        };
        let caption = match (tile.sent, tile.stills) {
            (true, _) => "Sent · click to reopen".to_string(),
            (false, 1) => "Not sent · 1 still".to_string(),
            (false, stills) => format!("Not sent · {stills} stills"),
        };
        col()
            .key("tile")
            .hit()
            .w_full()
            .h_full()
            .padding(8.)
            .gap(6.)
            .radius(k.radius.xl + 2.)
            .fill(k.c.bg_base)
            .border(1., crate::kit::hud_edge())
            .on_hover_change(|s: &mut Tile, inside, _| s.lens.hover_tile(inside))
            .on_press(|s: &mut Tile| s.lens.reopen_tile())
            .child(
                el().grow(1.)
                    .w_full()
                    .radius(k.radius.md)
                    .fill(k.c.layer_01)
                    .clip_rounded(k.radius.md)
                    .children(
                        tile.thumb
                            .as_ref()
                            .map(|thumb| el().w_full().h_full().pixels(thumb)),
                    ),
            )
            .child(
                text(caption)
                    .key("tile-caption")
                    .font(k.ty.small_medium)
                    .color(k.c.text_muted),
            )
            .child(
                el().key("tile-discard")
                    .label("Discard this capture")
                    .absolute(SIZE.0 - 24., 4.)
                    .size(20., 20.)
                    .center()
                    .radius(10.)
                    .fill(Color::BLACK.alpha(0.6))
                    .child(text("×").font(k.ty.small_medium).color(Color::WHITE))
                    .on_press(|s: &mut Tile| s.lens.discard_tile()),
            )
    })
}
