//! The tool strip: while the draw key is down, the six tools fan out on an arc
//! around the point where the pointer was when it went down, like ink thrown from
//! it. Each shows, in ink, the mark it makes, and the current one stands out. A
//! click on one, or fn with its digit, picks it. The strip is drawn on the ink
//! layer, so it comes and goes with the key and never holds up a stroke: a press
//! anywhere else on the layer draws at once, and no chip covers the pointer.
use super::ink::{self, Ink, InkTool};
use crate::document::{Mark, Tool};
use rune::{Color, El, Shape, TextStyle, Transition, canvas, el, text};
use rune_kit::Kit;

const GLYPH: f32 = 20.;
const GLYPH_WIDTH: f32 = 2.2;
const CHIP: f32 = 30.;
/// The current chip is this much larger.
const CURRENT_SCALE: f32 = 1.22;
/// The name's pill, beyond the current chip.
const NAME: (f32, f32) = (120., 20.);

/// Where the arc points, how wide it is and how far out, in degrees and points.
/// Straight up when it fits; near an edge, the first that keeps every chip in
/// the layer. A corner only has a quarter turn, so the arc there is narrower and
/// further out, with the chips still apart.
const ARCS: [(f32, f32, f32); 12] = [
    (-90., 150., 84.),
    (90., 150., 84.),
    (0., 150., 84.),
    (180., 150., 84.),
    (-45., 150., 84.),
    (-135., 150., 84.),
    (45., 150., 84.),
    (135., 150., 84.),
    (-45., 80., 120.),
    (-135., 80., 120.),
    (45., 80., 120.),
    (135., 80., 120.),
];

/// Each chip's centre for an arc around `at`, in tool order.
fn chips(at: (f32, f32), (towards, span, radius): (f32, f32, f32)) -> [(f32, f32); 6] {
    std::array::from_fn(|i| {
        let angle = (towards - span / 2. + span * i as f32 / 5.).to_radians();
        (at.0 + radius * angle.cos(), at.1 + radius * angle.sin())
    })
}

fn fits(centres: &[(f32, f32)], size: (f32, f32)) -> bool {
    let half = CHIP * CURRENT_SCALE / 2.;
    centres
        .iter()
        .all(|&(x, y)| x - half >= 0. && y - half >= 0. && x + half <= size.0 && y + half <= size.1)
}

/// The mark `tool` makes, small, in a `GLYPH` square.
fn sample(tool: InkTool, color: [u8; 4]) -> Mark {
    let s = GLYPH;
    let (kind, points, text) = match tool {
        InkTool::Pen => (
            Tool::Pen,
            (0..=12)
                .map(|i| {
                    let x = 2. + i as f32 * (s - 4.) / 12.;
                    (x, s / 2. + (i as f32 * 0.9).sin() * s * 0.22)
                })
                .collect(),
            "",
        ),
        InkTool::Arrow => (Tool::Arrow, vec![(3., s - 3.), (s - 3., 3.)], ""),
        InkTool::Box => (Tool::Rectangle, vec![(3., 5.), (s - 3., s - 5.)], ""),
        InkTool::Highlight => (Tool::Highlight, vec![(1., 6.), (s - 1., s - 6.)], ""),
        InkTool::Label => (Tool::Text, vec![(2., 2.)], "Aa"),
        InkTool::Counter => (Tool::Counter, vec![(s / 2., s / 2.)], "1"),
    };
    Mark {
        style: Default::default(),
        tool: kind,
        curve: None,
        points,
        color,
        width: GLYPH_WIDTH,
        text: text.into(),
        drawn: None,
    }
}

/// `tool`'s mark in `color`, as the ink layer draws it.
fn glyph(tool: InkTool, color: [u8; 4], counter_text: Color) -> El {
    let mark = sample(tool, color);
    let base = el().size(GLYPH, GLYPH);
    match tool {
        InkTool::Label => base.center().child(
            text("Aa")
                .font(TextStyle::new(14., 700.))
                .color(ink::color(color, 1.)),
        ),
        InkTool::Counter => base
            .child(
                canvas(ink::draw(Shape::new(), &mark, (0., 0.)))
                    .absolute(0., 0.)
                    .size(GLYPH, GLYPH),
            )
            .child(
                el().absolute(0., 0.).size(GLYPH, GLYPH).center().child(
                    text("1")
                        .font(TextStyle::new(11., 700.))
                        .color(counter_text),
                ),
            ),
        _ => base.child(
            canvas(ink::draw(Shape::new(), &mark, (0., 0.)))
                .absolute(0., 0.)
                .size(GLYPH, GLYPH),
        ),
    }
}

/// One tool's hit target. A press picks the tool and draws nothing.
fn cell(tool: InkTool, size: f32) -> El {
    el().key(format!("tool-{}", tool.key()))
        .label(tool.name())
        .size(size, size)
        .center()
        .on_press(move |s: &mut Ink| s.lens.pick_tool(tool))
}

/// The strip for `current`, its pointer at `at` in the layer's points, inside a
/// layer of `size`. The chips fan out around the pointer, never over it, so the
/// stroke starts clear; the arc turns away from an edge to stay on screen.
/// `drawing` while a stroke is under way: the strip steps back so the ink reads first.
/// The fan is drawn (D4 on `s-h47c5h`); its chips and the name take the HUD's kit.
pub fn tool_strip(
    current: InkTool,
    at: (f32, f32),
    size: (f32, f32),
    drawing: bool,
    k: &Kit,
) -> El {
    let ink_rgba = ink::INK;
    let ink_color = ink::color(ink_rgba, 1.);
    let white = [255, 255, 255, 255];
    let arc = ARCS
        .into_iter()
        .find(|&arc| fits(&chips(at, arc), size))
        .unwrap_or(ARCS[0]);
    let centres = chips(at, arc);
    let mut fan = el()
        .key("tool-strip")
        .label(current.name())
        .absolute(0., 0.)
        .size(size.0, size.1)
        .opacity(if drawing { 0.3 } else { 1. })
        .transition(Transition::opacity());
    for (i, (&tool, &(cx, cy))) in InkTool::ALL.iter().zip(&centres).enumerate() {
        let on = tool == current;
        let chip = cell(tool, CHIP)
            .absolute(cx - CHIP / 2., cy - CHIP / 2.)
            .radius(CHIP / 2.)
            .fill(if on { ink_color } else { k.c.bg_base })
            .border(
                1.,
                if on {
                    ink_color
                } else {
                    crate::kit::hud_edge()
                },
            );
        let chip = k
            .elevate(chip, &k.elevation.floating)
            .scale(if on { CURRENT_SCALE } else { 1. })
            .child(glyph(
                tool,
                if on { white } else { ink_rgba },
                if on { ink_color } else { Color::WHITE },
            ))
            // Thrown out from the pointer, each a beat after the last.
            .transition(
                Transition::scale(0.4)
                    .combined(Transition::offset((at.0 - cx) * 0.6, (at.1 - cy) * 0.6)),
            )
            .enter_delay(i as f32 * 0.012);
        fan = fan.child(chip);
    }
    // The name sits past the arc's middle, away from the pointer.
    let (towards, _, radius) = arc;
    let (dx, dy) = (towards.to_radians().cos(), towards.to_radians().sin());
    let out =
        radius + CHIP * CURRENT_SCALE / 2. + 6. + dx.abs() * NAME.0 / 2. + dy.abs() * NAME.1 / 2.;
    let (nx, ny) = (at.0 + dx * out, at.1 + dy * out);
    let name_x = (nx - NAME.0 / 2.).clamp(0., (size.0 - NAME.0).max(0.));
    let name_y = (ny - NAME.1 / 2.).clamp(0., (size.1 - NAME.1).max(0.));
    fan.child(
        el().key("tool-name")
            .absolute(name_x, name_y)
            .size(NAME.0, NAME.1)
            .center()
            .child(
                el().px(8.)
                    .py(2.)
                    .radius(k.radius.md)
                    .fill(k.c.bg_base)
                    .border(1., crate::kit::hud_edge())
                    .child(
                        text(format!("{} · fn {}", current.name(), current.digit()))
                            .font(k.ty.small_medium)
                            .color(k.c.text_base),
                    ),
            ),
    )
}
