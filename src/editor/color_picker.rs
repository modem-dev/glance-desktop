//! A hue and saturation wheel, a value slider and a hex field. Opacity belongs to the
//! tool and stays as it is.
use super::{Editor, actions::Action, live::rgba};
use rune::{Cx, Drag, El, Key, Phase, Pixels, TextEdit, Tx, col, el, row, text};
use rune_kit::TextInput;

const WHEEL: f32 = 148.;
const WHEEL_PIXELS: u32 = 296;

/// `#RGB` or `#RRGGBB`, with or without the `#`.
pub(crate) fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let text = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !text.is_ascii() || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match text.len() {
        3 => Some(std::array::from_fn(|i| {
            u8::from_str_radix(&text[i..i + 1], 16).unwrap() * 17
        })),
        6 => Some(std::array::from_fn(|i| {
            u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()
        })),
        _ => None,
    }
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn hsv([r, g, b]: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = [r, g, b].map(|c| c as f32 / 255.);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0. {
        0.
    } else if max == r {
        ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    } / 6.;
    [h, if max == 0. { 0. } else { d / max }, max]
}

fn from_hsv([h, s, v]: [f32; 3]) -> [u8; 3] {
    let h = h.rem_euclid(1.) * 6.;
    let c = v * s;
    let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
    let rgb = match h as u32 {
        0 => [c, x, 0.],
        1 => [x, c, 0.],
        2 => [0., c, x],
        3 => [0., x, c],
        4 => [x, 0., c],
        _ => [c, 0., x],
    };
    rgb.map(|a| ((a + v - c) * 255.).round() as u8)
}

fn wheel_pixels() -> Pixels {
    let pixels = Pixels::new();
    let half = WHEEL_PIXELS as f32 / 2.;
    let mut rgba = Vec::with_capacity((WHEEL_PIXELS * WHEEL_PIXELS * 4) as usize);
    for y in 0..WHEEL_PIXELS {
        for x in 0..WHEEL_PIXELS {
            let dx = (x as f32 + 0.5 - half) / (half - 2.);
            let dy = (y as f32 + 0.5 - half) / (half - 2.);
            let radius = dx.hypot(dy);
            let [r, g, b] = from_hsv([dy.atan2(dx) / std::f32::consts::TAU, radius.min(1.), 1.]);
            let alpha = ((1. - radius) * (half - 2.) + 0.5).clamp(0., 1.);
            rgba.extend([r, g, b, (alpha * 255.).round() as u8]);
        }
    }
    pixels.set(WHEEL_PIXELS, WHEEL_PIXELS, rgba);
    pixels
}

pub(crate) struct ColorPicker {
    pub open: bool,
    /// Which backdrop colour the picker edits; `None` edits marks.
    pub stop: Option<usize>,
    hsv: [f32; 3],
    /// During a drag on the wheel or slider, every move after the first shares one undo step.
    pub continuing: bool,
    pub hex: TextEdit,
    pub hex_invalid: bool,
    wheel: Pixels,
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self {
            open: false,
            stop: None,
            hsv: [0., 1., 1.],
            continuing: false,
            hex: TextEdit::new(""),
            hex_invalid: false,
            wheel: wheel_pixels(),
        }
    }
}

impl ColorPicker {
    /// Follows a colour chosen elsewhere. A grey or black keeps the wheel's hue.
    pub fn sync(&mut self, rgb: [u8; 3]) {
        if from_hsv(self.hsv) != rgb {
            let [h, s, v] = hsv(rgb);
            self.hsv = [if s > 0. { h } else { self.hsv[0] }, s, v];
        }
        self.hex = TextEdit::new(hex(rgb));
        self.hex_invalid = false;
    }
}

impl Editor {
    /// An open backdrop picker shows its colour as it is after any action, such as a
    /// palette or undo. It closes when the background stops showing that colour.
    pub(crate) fn follow_backdrop_picker(&mut self) {
        let Some(stop) = self.picker.stop.filter(|_| self.picker.open) else {
            return;
        };
        let b = self.current_backdrop();
        if stop >= b.stops() {
            self.picker.open = false;
            return;
        }
        let color = b.colors()[stop];
        let rgb = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
        // The wheel's own drags already match; leave the hex field being typed alone.
        if from_hsv(self.picker.hsv) != rgb {
            self.picker.sync(rgb);
        }
    }

    fn apply_picked(&mut self, drag: &Drag, tx: &mut Tx) {
        let rgb = from_hsv(self.picker.hsv);
        if matches!(drag.phase, Phase::Down) {
            self.picker.continuing = false;
        }
        let revision = self.revision;
        let action = self.picked(rgb, !self.picker.continuing);
        self.dispatch_ui(action, tx);
        // The undo step starts with the first move that changes something: a press at the
        // current colour saves nothing, so later moves have to.
        let started = self.picker.continuing || self.revision != revision;
        self.picker.continuing = started && !matches!(drag.phase, Phase::Up);
        self.picker.hex = TextEdit::new(hex(rgb));
    }

    /// What a picked colour changes: the backdrop colour being edited, or the marks.
    pub(crate) fn picked(&self, rgb: [u8; 3], remember: bool) -> Action {
        match self.picker.stop {
            Some(stop) => Action::SetBackdropColor {
                stop,
                rgb,
                remember,
            },
            None => Action::SetRgb(rgb),
        }
    }

    fn on_wheel_drag(&mut self, drag: &mut Drag, tx: &mut Tx) {
        let half = WHEEL / 2.;
        let (dx, dy) = ((drag.x - half) / half, (drag.y - half) / half);
        self.picker.hsv[0] = dy.atan2(dx) / std::f32::consts::TAU;
        self.picker.hsv[1] = dx.hypot(dy).min(1.);
        self.apply_picked(drag, tx);
    }

    fn on_value_drag(&mut self, drag: &mut Drag, tx: &mut Tx) {
        self.picker.hsv[2] = (drag.x / WHEEL).clamp(0., 1.);
        self.apply_picked(drag, tx);
    }

    pub(super) fn apply_hex(&mut self, tx: &mut Tx) {
        match parse_hex(&self.picker.hex.text) {
            Some(rgb) => {
                let action = self.picked(rgb, true);
                self.dispatch_ui(action, tx);
            }
            None => self.picker.hex_invalid = true,
        }
    }

    pub(crate) fn color_picker_view(&self, cx: &Cx<Editor>) -> El {
        let k = super::panels::controls::kit(cx);
        let [h, s, v] = self.picker.hsv;
        let half = WHEEL / 2.;
        let angle = h * std::f32::consts::TAU;
        let bright = from_hsv([h, s, 1.]);
        // The field takes what Apply leaves of the row.
        let hex_field = TextInput::new(&self.picker.hex)
            .key("hex-field")
            .label("Hex color")
            .invalid(self.picker.hex_invalid)
            .on_edit(|s: &mut Editor| &mut s.picker.hex)
            .on_submit(|s: &mut Editor, tx| s.apply_hex(tx))
            .on_key(|s: &mut Editor, key, tx| {
                if key.key == Key::Escape {
                    s.picker.open = false;
                    tx.blur();
                    return true;
                }
                false
            })
            .build(k)
            .shrink()
            .min_w(0.);
        let mut picker = col()
            .key("color-picker")
            .gap(8.)
            .child(
                el().key("color-wheel")
                    .size(WHEEL, WHEEL)
                    .pixels(&self.picker.wheel)
                    .role("slider")
                    .label("Hue and saturation")
                    .cursor(rune::Cursor::Crosshair)
                    .on_drag(|s: &mut Editor, drag, tx| s.on_wheel_drag(drag, tx))
                    .child(
                        el().absolute(
                            half + angle.cos() * s * half - 5.,
                            half + angle.sin() * s * half - 5.,
                        )
                        .size(10., 10.)
                        .radius(5.)
                        .border(2., rgba([255, 255, 255, 255]))
                        .inert(),
                    ),
            )
            .child(
                el().key("color-value")
                    .w(WHEEL)
                    .h(14.)
                    .radius(7.)
                    .fill(rgba([bright[0], bright[1], bright[2], 255]))
                    .role("slider")
                    .label("Brightness")
                    .range_value(v as f64, 0., 1.)
                    .on_drag(|s: &mut Editor, drag, tx| s.on_value_drag(drag, tx))
                    .child(
                        el().absolute(v * WHEEL - 5., 2.)
                            .size(10., 10.)
                            .radius(5.)
                            .border(2., rgba([255, 255, 255, 255]))
                            .inert(),
                    ),
            )
            .child(
                row()
                    .w(super::panels::controls::INNER)
                    .gap(6.)
                    .center_y()
                    .child(hex_field)
                    .child(super::panels::controls::button(
                        cx,
                        "hex-apply",
                        "Apply",
                        true,
                        |s: &mut Editor, tx| s.apply_hex(tx),
                    )),
            );
        if self.picker.hex_invalid {
            picker = picker.child(
                text("Use #RGB or #RRGGBB.")
                    .key("hex-error")
                    .font(k.ty.ui)
                    .color(k.c.danger.fg),
            );
        }
        picker
    }
}

#[cfg(test)]
mod tests {
    use super::{from_hsv, hsv, parse_hex};

    #[test]
    fn hex_validation_and_hsv_round_trip() {
        assert_eq!(parse_hex(" #0aF "), Some([0, 170, 255]));
        assert_eq!(parse_hex("abcdef"), Some([171, 205, 239]));
        for text in ["", "#", "#12", "#abcd", "#11223344", "＃123456", "#xyzxyz"] {
            assert_eq!(parse_hex(text), None);
        }
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    assert_eq!(from_hsv(hsv([r, g, b])), [r, g, b]);
                }
            }
        }
    }
}
