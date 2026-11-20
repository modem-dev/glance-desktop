//! The right sidebar: the selected mark's options, otherwise the active tool's, or
//! the image tools.
mod animation;
mod backdrop;
pub(crate) mod controls;
mod image;
mod tools;

use super::{Editor, NumberDraft, Panel};
use rune::{Cx, El, Tx, col};

pub(crate) const WIDTH: f32 = 272.;

/// Minimum, maximum and step of each numeric field.
pub(crate) fn number_limits(label: &str) -> (f32, f32, f32) {
    match label {
        "Thickness" => (1., 32., 1.),
        "Font size" => (7., 112., 1.),
        "Block size" => (12., 128., 4.),
        "Badge diameter" => (24., 144., 7.2),
        "Number" | "Next number" => (1., 999., 1.),
        "Lens diameter" => (48., 600., 24.),
        "Dim surroundings" => (0., 95., 5.),
        "Opacity" | "Intensity" => (10., 100., 10.),
        "Corner radius" => (0., 128., 4.),
        _ => unreachable!("registered numeric field {label}"),
    }
}

pub(crate) fn parse_number(text: &str, limits: (f32, f32, f32)) -> Option<f32> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && (limits.0..=limits.1).contains(value))
}

pub(crate) fn shown_number(value: f32) -> String {
    let rounded = (value * 10.).round() / 10.;
    if rounded.fract() == 0. {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

impl Editor {
    pub(crate) fn begin_number(&mut self, label: &'static str, value: f32, tx: &mut Tx) {
        self.commit_text(tx);
        self.commit_number(tx);
        self.number = Some(NumberDraft { label });
        let shown = shown_number(value);
        let end = shown.len();
        self.number_edit = rune::TextEdit::new(shown);
        self.number_edit.set_selection(0, end);
        self.number_edit.focused = true;
    }

    pub(crate) fn sidebar(&self, cx: &mut Cx<Editor>, layer: &mut controls::Layer) -> El {
        let k = controls::kit(cx);
        col()
            .key("sidebar")
            .w(WIDTH)
            .h_full()
            .fill(k.c.bg_base)
            .border(1., k.c.border_muted)
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            })
            .child(
                col()
                    .w_full()
                    .gap(14.)
                    .px(14.)
                    .py(14.)
                    .child(match self.panel {
                        Panel::Tool => self.tool_controls(cx, layer),
                        Panel::Image => self.image_controls(cx, layer),
                        Panel::Backdrop => self.backdrop_controls(cx, layer),
                        Panel::Animation => self.animation_controls(cx, layer),
                    }),
            )
    }
}
