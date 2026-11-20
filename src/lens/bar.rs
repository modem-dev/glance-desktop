//! The bar beside the area: the session chip, the message, voice, the still
//! strip, Edit and Send, and the Tab palette. It takes keys without
//! activating Lens, and shows who has them (D6 on the plan).
use super::{
    AttnStatus, KeyOwner, Lens, SendStatus, Summary, VoiceState, attn,
    session::{DisplayFrame, HostTime},
};
use crate::platform_port::VoiceStatus;
use image::RgbaImage;
use rune::time::Instant;
use rune::{Color, Cx, Drag, El, Key, KeyEvent, Phase, TextEdit, Tx, col, el, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, TextInput};
use std::sync::Arc;

pub const WIDTH: f32 = 760.;
/// Every control on the bar stands this tall: the kit's large push button.
const CONTROL: f32 = 28.;
/// The chip's widest, so the message field keeps most of the row.
const CHIP_WIDTH: f32 = 260.;
const GAP: f32 = 8.;
const PADDING: f32 = 10.;
const STATUS: f32 = 44.;
const PALETTE_ROW: f32 = 36.;
/// How many rows the palette shows before it scrolls.
const PALETTE_ROWS: usize = 8;
const THUMB: (u32, u32) = (104, 64);
/// One frame of a headless run: how far the press lags the focus it caused where
/// input carries no arrival time (measured: 16.67 ms of script time).
const UNSTAMPED_FRAME: std::time::Duration = std::time::Duration::from_micros(16_667);
/// Keeps the bar off the area and the display's edge.
const MARGIN: f64 = 8.;

/// A still's small picture for the strip.
pub fn thumbnail(frame: &RgbaImage) -> rune::Pixels {
    let pixels = rune::Pixels::new();
    draw_thumbnail(&pixels, frame);
    pixels
}

/// Draws `frame` small into `pixels`, which a strip may already be showing.
pub fn draw_thumbnail(pixels: &rune::Pixels, frame: &RgbaImage) {
    let scale = (f64::from(THUMB.0) / f64::from(frame.width().max(1)))
        .min(f64::from(THUMB.1) / f64::from(frame.height().max(1)));
    let width = ((f64::from(frame.width()) * scale) as u32).max(1);
    let height = ((f64::from(frame.height()) * scale) as u32).max(1);
    let small = image::imageops::thumbnail(frame, width, height);
    pixels.set(width, height, small.into_raw());
}

/// What stops or follows a send, for the line under the controls.
fn status(summary: &Summary) -> Option<String> {
    match &summary.attn {
        AttnStatus::Unpaired(why) => {
            return Some(format!(
                "{why} Press ↩ to pair: attn asks you to allow “{}”.",
                attn::SENDER_NAME
            ));
        }
        AttnStatus::Pairing(None) => return Some("Asking attn to pair…".into()),
        AttnStatus::Pairing(Some(code)) => {
            return Some(format!(
                "In attn, check the code is {code} and click Allow."
            ));
        }
        AttnStatus::Unlisted(why) => {
            return Some(format!("{why} Press ↩ to list the sessions again."));
        }
        AttnStatus::Loading | AttnStatus::Ready => {}
    }
    match &summary.send {
        SendStatus::Sending => Some("Sending…".into()),
        SendStatus::Failed(error) => Some(error.clone()),
        SendStatus::Idle | SendStatus::Sent(_) => None,
    }
}

/// The bar's height for what it shows now.
pub fn height(summary: &Summary) -> f32 {
    let mut height = PADDING * 2. + CONTROL * 2. + GAP;
    if status(summary).is_some() || summary.error.is_some() {
        height += STATUS;
    }
    if summary.palette.is_some() {
        height += palette_height(summary);
    }
    height
}

fn palette_height(summary: &Summary) -> f32 {
    PALETTE_ROW * summary.sessions.len().clamp(1, PALETTE_ROWS) as f32 + GAP
}

/// Where the bar goes: where the user dragged it, or under the area if it fits,
/// then over it, then (a whole display) inside the bottom edge. Either way it stays
/// whole on the display, clear of the menu bar and the Dock. Stills leave Lens's
/// windows out.
pub fn placement(
    area: [f64; 4],
    display: DisplayFrame,
    height: f32,
    dragged: Option<(f64, f64)>,
) -> [f32; 4] {
    let (display_w, display_h) = (display.frame[2], display.frame[3]);
    let width = f64::from(WIDTH).min(display_w - MARGIN * 2.);
    let height = f64::from(height);
    let right = (display_w - width - MARGIN).max(MARGIN);
    let top = display.menu_bar + MARGIN;
    let bottom = (display_h - display.dock - height - MARGIN).max(top);
    let (x, y) = dragged.unwrap_or_else(|| {
        let below = area[1] + area[3] + MARGIN;
        let above = area[1] - MARGIN - height;
        let y = if below <= bottom {
            below
        } else if above >= top {
            above
        } else {
            bottom
        };
        (area[0], y)
    });
    let (x, y) = (x.clamp(MARGIN, right), y.clamp(top, bottom));
    [x as f32, y as f32, width as f32, height as f32]
}

/// A drag on the bar's background: where the press sat in the bar, so the bar
/// moves with the pointer and does not jump.
#[derive(Clone, Copy)]
struct Grab {
    offset: (f32, f32),
    /// Where the bar was, for a drag that is cancelled.
    was: Option<(f64, f64)>,
    /// The press took the keyboard from the page, which gets it back when the drag ends.
    from_page: bool,
}

pub struct Bar {
    pub lens: Arc<Lens>,
    pub seen: u64,
    capture: Option<u64>,
    /// The bar's top-left in points from the primary display's top-left, as the
    /// window last reported it.
    origin: (f32, f32),
    grab: Option<Grab>,
    /// When the bar took the keys from the page, which only a press on it does: by
    /// Rune's clock, and by the port's for a host that does not stamp input.
    pub keys_from_page: Option<(Instant, HostTime)>,
    placing_label: bool,
    message: TextEdit,
    label: TextEdit,
}

impl Bar {
    pub fn new(lens: Arc<Lens>) -> Self {
        let mut message = TextEdit::new("");
        message.focused = true;
        Self {
            lens,
            seen: 0,
            capture: None,
            origin: (0., 0.),
            grab: None,
            keys_from_page: None,
            placing_label: false,
            message,
            label: TextEdit::new(""),
        }
    }

    /// Follows the shared state: a new capture starts with an empty message, and
    /// placing a label moves the typing to its field.
    /// A label being placed is finished first: the editor is about to take the still.
    fn finish_label(&mut self) {
        if self.placing_label {
            self.lens.finish_label(&self.label.text);
        }
    }

    pub fn follow(&mut self) {
        let (capture, message, placing) = self.lens.read(|summary| {
            let capture = summary.capture.as_ref();
            (
                capture.map(|c| c.id),
                capture.map_or(String::new(), |c| c.message.clone()),
                summary.placing_label,
            )
        });
        if capture != self.capture {
            self.capture = capture;
            self.message = TextEdit::new(&message);
        }
        if placing != self.placing_label {
            self.placing_label = placing;
            self.label = TextEdit::new("");
        }
        self.label.focused = placing;
        self.message.focused = !placing;
    }
}

/// Send is the bar's one primary action; Edit and the rest are neutral.
fn button(key: &str, content: String, primary: bool, k: &Kit) -> El {
    Button::new(content)
        .variant(if primary {
            ButtonVariant::Submit
        } else {
            ButtonVariant::Neutral
        })
        .control_size(ControlSize::Large)
        .build(k)
        .key(key.to_string())
}

/// What the chip says: the target session, and where it runs once there is one.
fn chip_parts(summary: &Summary) -> (String, Option<String>) {
    let target = summary
        .capture
        .as_ref()
        .and_then(|c| c.target.as_ref())
        .and_then(|id| summary.sessions.iter().find(|s| &s.id == id));
    let why = match (&summary.attn, target) {
        (AttnStatus::Unpaired(_) | AttnStatus::Pairing(_), _) => "Not paired with attn",
        (AttnStatus::Unlisted(_), _) => "Sessions unavailable",
        (_, Some(session)) => {
            return (format!("→ {}", session.label), Some(session.host_label()));
        }
        (AttnStatus::Loading, None) => "Loading sessions…",
        (AttnStatus::Ready, None) if summary.sessions.is_empty() => "No sessions in attn",
        (AttnStatus::Ready, None) => "Choose a session",
    };
    (why.into(), None)
}

/// The session chip. A long session label ends in "…" inside the chip, and the
/// host after it stays whole; the full text is the chip's label and, while cut, its tooltip.
fn chip(summary: &Summary, k: &Kit) -> El {
    let (target, host) = chip_parts(summary);
    let full = match &host {
        Some(host) => format!("{target} · {host}"),
        None => target.clone(),
    };
    let tooltip = full.clone();
    let faint = k.c.text_faint;
    Button::new(full.clone())
        .control_size(ControlSize::Large)
        .expanded(summary.palette.is_some())
        .label_with(move |style, ink| {
            row()
                .shrink()
                .min_w(0.)
                .gap(4.)
                .center_y()
                .child(
                    text(target)
                        .key("chip-target")
                        .shrink()
                        .font(style)
                        .color(ink)
                        .truncate()
                        .help_when_truncated(tooltip),
                )
                .children(host.map(|host| {
                    text(format!("· {host}"))
                        .key("chip-host")
                        .font(style)
                        .color(faint)
                }))
        })
        .on_press(|s: &mut Bar| s.lens.toggle_palette())
        .build(k)
        .key("chip")
        .label(full)
        .shrink()
        .min_w(0.)
        .max_w(CHIP_WIDTH)
}

fn strip(summary: &Summary, k: &Kit) -> El {
    let mut strip = row().key("strip").grow(1.).h(CONTROL).gap(6.).scroll_x();
    let stills = summary.capture.as_ref().map_or(&[][..], |c| &c.stills);
    for (index, still) in stills.iter().enumerate() {
        let id = still.id;
        strip = strip.child(
            el().key(format!("still-{index}"))
                .label(format!(
                    "{:?} {}×{}",
                    still.cause, still.width, still.height
                ))
                .size(48., CONTROL)
                .radius(k.radius.sm)
                .fill(k.c.layer_01)
                .border(1., k.c.border_base)
                .clip_rounded(k.radius.sm)
                .on_press(move |s: &mut Bar| {
                    s.finish_label();
                    s.lens.edit_still(id);
                })
                .child(el().w_full().h_full().pixels(&still.thumb))
                .child(
                    el().key(format!("drop-{index}"))
                        .label("Drop this still")
                        .absolute(32., 0.)
                        .size(16., 16.)
                        .center()
                        .radius(8.)
                        .fill(Color::BLACK.alpha(0.6))
                        .child(text("×").font(k.ty.small_medium).color(Color::WHITE))
                        .on_press(move |s: &mut Bar| s.lens.drop_still(id)),
                ),
        );
    }
    strip
}

/// The session palette: rows in the bar's own layout, styled as the kit's PopupList
/// rows, because a kit popover draws inside its window and the bar is a small one (F12).
fn palette(summary: &Summary, selected: usize, k: &Kit) -> El {
    let mut list = col()
        .key("palette")
        .w_full()
        .h(palette_height(summary) - GAP)
        .scroll(rune::ScrollSpec {
            stick_bottom: false,
        });
    if summary.sessions.is_empty() {
        let empty = match &summary.attn {
            AttnStatus::Loading => "Loading sessions…",
            AttnStatus::Ready => "No live sessions in attn. Start one there.",
            _ => "Attn Lens cannot list the sessions; the line above says why.",
        };
        return list.child(
            row().h(PALETTE_ROW).px(8.).center_y().child(
                text(empty)
                    .key("palette-empty")
                    .font(k.ty.ui)
                    .color(k.c.text_muted),
            ),
        );
    }
    for (index, session) in summary.sessions.iter().enumerate() {
        list = list.child(
            row()
                .key(format!("palette-row-{index}"))
                .w_full()
                .h(PALETTE_ROW)
                .px(8.)
                .gap(12.)
                .center_y()
                .radius(k.radius.md)
                .fill(if index == selected {
                    k.c.surface_raised_hover
                } else {
                    Color::TRANSPARENT
                })
                .hover(k.c.hover)
                .child(
                    text(session.label.clone())
                        .key(format!("palette-label-{index}"))
                        .grow(1.)
                        .shrink()
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .truncate()
                        .help_when_truncated(session.label.clone()),
                )
                .child(
                    text(format!(
                        "{} · {} · {}",
                        session.agent,
                        session.state_label(),
                        session.host_label()
                    ))
                    .key(format!("palette-meta-{index}"))
                    .font(k.ty.ui)
                    .color(k.c.text_faint),
                )
                .on_press(move |s: &mut Bar| s.lens.choose_session(Some(index))),
        );
    }
    list
}

/// Enter, Esc, Tab and the palette's arrows. The field hands them here when it has
/// focus; the bar window does when anything else in the bar has. Returns whether the
/// key was used.
pub fn bar_key(s: &mut Bar, event: &KeyEvent, _: &mut Tx) -> bool {
    let (palette_open, placing) = s
        .lens
        .read(|summary| (summary.palette.is_some(), summary.placing_label));
    match event.key {
        Key::Enter if placing => s.lens.finish_label(&s.label.text),
        Key::Escape if placing => s.lens.finish_label(""),
        Key::Enter if palette_open => s.lens.choose_session(None),
        Key::Escape if palette_open => s.lens.toggle_palette(),
        Key::Up if palette_open => s.lens.move_palette(-1),
        Key::Down if palette_open => s.lens.move_palette(1),
        Key::Enter => s.lens.send_or_recover(),
        Key::Escape => s.lens.end_capture(),
        Key::Tab => s.lens.toggle_palette(),
        _ => return false,
    }
    true
}

pub fn bar_view(s: &Bar, _: &mut Cx<Bar>) -> El {
    let k = crate::kit::hud();
    s.lens.read(|summary: &Summary| {
        let keys_here = summary.keys == KeyOwner::Bar;
        // The field draws no box of its own: the row around it shows who has the keys.
        let field = if summary.placing_label {
            TextInput::new(&s.label)
                .key("label")
                .placeholder("Label text, then ↩")
                .on_edit(|s: &mut Bar| &mut s.label)
        } else {
            TextInput::new(&s.message)
                .key("message")
                .placeholder(if keys_here {
                    "Say what you mean"
                } else {
                    "The page has the keys · ⌘⌥C to type here"
                })
                .on_edit(|s: &mut Bar| &mut s.message)
                .on_change(|s: &mut Bar, _: &mut Tx| {
                    s.lens.set_message(&s.message.text);
                })
        }
        .plain()
        .width(None)
        .height(CONTROL - 2.)
        .text_style(k.ty.ui)
        .on_key(bar_key)
        .build(k);

        let voice = voice_text(summary.voice);
        let segments = summary.capture.as_ref().map_or(&[][..], |c| &c.segments);
        let controls = row()
            .w_full()
            .h(CONTROL)
            .gap(GAP)
            .center_y()
            .child(chip(summary, k))
            .child(
                row()
                    .key("field")
                    .grow(1.)
                    .min_w(0.)
                    .h(CONTROL)
                    .px(8.)
                    .center_y()
                    .radius(k.radius.lg)
                    .fill(k.c.bg_deep)
                    .border(
                        1.,
                        if keys_here {
                            k.c.text_accent
                        } else {
                            k.c.border_base
                        },
                    )
                    .child(field)
                    // A press inside the border is a press on the field, not a drag.
                    .on_press(|_: &mut Bar| {}),
            )
            .child(
                text(if keys_here {
                    "Typing here"
                } else {
                    "Typing on the page"
                })
                .key("keys")
                .font(k.ty.small_medium)
                .color(if keys_here {
                    k.c.text_accent
                } else {
                    k.c.text_faint
                }),
            );
        let stills = row()
            .w_full()
            .h(CONTROL)
            .gap(GAP)
            .center_y()
            .child(strip(summary, k))
            .children(segments.last().map(|segment| {
                text(segment.text.clone())
                    .key(format!("segment-{}", segments.len() - 1))
                    .max_w(160.)
                    .shrink()
                    .truncate()
                    .font(k.ty.small)
                    .color(k.c.text_muted)
            }))
            .child(
                text(voice)
                    .key("voice")
                    .shrink()
                    .truncate()
                    .font(k.ty.small_medium)
                    .color(if summary.voice == VoiceState::Off {
                        k.c.text_faint
                    } else {
                        k.c.text_accent
                    }),
            )
            .child(
                button("edit", "Edit".into(), false, k).on_press(|s: &mut Bar| {
                    s.finish_label();
                    s.lens.edit_latest();
                }),
            )
            .child(
                button(
                    "send",
                    match summary.attn {
                        AttnStatus::Unpaired(_) => "Pair ↩".into(),
                        _ if summary.send == SendStatus::Sending => "Sending…".into(),
                        _ => "Send ↩".to_string(),
                    },
                    true,
                    k,
                )
                .on_press(|s: &mut Bar| s.lens.send_or_recover()),
            );
        let mut bar = col()
            .key("bar")
            .w_full()
            .h_full()
            .padding(PADDING)
            .gap(GAP)
            .radius(k.radius.xl + 2.)
            .fill(k.c.bg_base)
            .border(1., crate::kit::hud_edge())
            .child(controls)
            .child(stills);
        let line = status(summary);
        if let Some(line) = &line {
            let failed = matches!(summary.send, SendStatus::Failed(_));
            bar = bar.child(
                text(line.clone())
                    .key("status")
                    .font(if failed { k.ty.ui_medium } else { k.ty.ui })
                    .color(if failed {
                        k.c.danger.fg
                    } else {
                        k.c.text_muted
                    }),
            );
        }
        // A failed send's reason is often the error itself: say it once.
        if let Some(error) = summary
            .error
            .as_ref()
            .filter(|error| line.as_ref() != Some(error))
        {
            bar = bar.child(
                text(error.clone())
                    .key("error")
                    .font(k.ty.ui_medium)
                    .color(k.c.danger.fg),
            );
        }
        if let Some(selected) = summary.palette {
            bar = bar.child(palette(summary, selected, k));
        }
        bar.on_screen_layout(|s: &mut Bar, rect| s.origin = (rect[0], rect[1]))
            .on_drag(drag_bar)
    })
}

/// A drag on the bar's background moves the bar: the field and the buttons take
/// their own presses. Where the pointer is comes from where the window is now, so
/// the bar follows it although the drag moves the window it is in.
fn drag_bar(s: &mut Bar, d: &mut Drag, tx: &mut Tx) {
    let capture = s
        .lens
        .read(|summary| summary.capture.as_ref().map(|c| (c.display, c.bar_at)));
    let Some((display, bar_at)) = capture else {
        return;
    };
    match d.phase {
        Phase::Down => {
            s.grab = Some(Grab {
                offset: (d.x, d.y),
                was: bar_at,
                // This press took them if it arrived before they moved: the focus
                // runs while the press that caused it is dispatched, and a later
                // press (into the field, then this drag) arrives after it. A host
                // that does not stamp input (headless) runs the press on the frame
                // after the focus.
                from_page: s.keys_from_page.take().is_some_and(|(focused, at)| {
                    tx.input_time().map_or_else(
                        || s.lens.port().now().since(at) <= UNSTAMPED_FRAME,
                        |pressed| pressed <= focused,
                    )
                }),
            });
        }
        Phase::Move => {
            if let Some(grab) = s.grab {
                let x = s.origin.0 + d.x - grab.offset.0;
                let y = s.origin.1 + d.y - grab.offset.1;
                s.lens.move_bar(Some((
                    f64::from(x) - display.frame[0],
                    f64::from(y) - display.frame[1],
                )));
            }
        }
        Phase::Up => {
            let Some(grab) = s.grab.take() else {
                return;
            };
            if d.cancelled() {
                s.lens.move_bar(grab.was);
            }
            if grab.from_page {
                s.lens.release_keys();
            }
        }
    }
}

/// Voice in the bar: what the voice key does now, or how far voice has got.
pub(super) fn voice_text(voice: VoiceState) -> String {
    match voice {
        VoiceState::Off => "right ⌥ voice".into(),
        VoiceState::Starting => "Starting voice…".into(),
        VoiceState::Running(VoiceStatus::Downloading { bytes, total }) => format!(
            "Downloading speech model {}%",
            bytes.saturating_mul(100) / total.max(1)
        ),
        VoiceState::Running(VoiceStatus::Loading) => "Loading speech model…".into(),
        VoiceState::Running(VoiceStatus::WarmingUp) => "Starting microphone…".into(),
        VoiceState::Running(VoiceStatus::Listening) => "Listening · right ⌥ to stop".into(),
        VoiceState::Finishing => "Finishing…".into(),
    }
}
