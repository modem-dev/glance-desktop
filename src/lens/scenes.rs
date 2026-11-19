//! The capture surfaces' scenes, for `--window bar|ink|tile|pill|picker-0 --scene NAME`:
//! one per screen and hard state. `scripts/snap.sh` renders them all.
//!
//! A scene reaches its state as a script does: it plays the port's script marks for
//! what comes from outside Lens (keys, the speech helper, a failing still), and calls
//! the method a window's own gesture calls for the rest (`Lens::pick` for the picker's
//! drag, `begin_stroke` and `finish_stroke` for the ink layer's). Every window shares one
//! `Lens`, so a scene sets up the whole capture and its window shows its part.
//!
//! Lens lists sessions and sends through the attn CLI. A scene runs the stand-in
//! `tools/scenes/attn`, which `scripts/snap.sh` names in `ATTN_BIN`: eight sessions,
//! one with a long label. A state that needs a job held in flight, such as a refining
//! pill, stays a mark in the `headless` scripts.
use super::{Lens, bar::Bar, ink::Ink, pill::Pill, select::Picker, tile::Tile};
use crate::app::Scenes;
use std::sync::Arc;

pub(crate) const BAR_SCENES: Scenes<Bar> = &[
    ("open", |s| capture(&s.lens)),
    ("highlighted", |s| highlighted(&s.lens)),
    ("three-stills", |s| three_stills(&s.lens)),
    ("palette", |s| palette(&s.lens)),
    ("long-label", |s| long_label(&s.lens)),
    ("unpaired", |s| unpaired(&s.lens)),
    ("voice-downloading", |s| {
        voice(&s.lens, "voice:downloading:182839392:731357568")
    }),
    ("voice-listening", |s| voice(&s.lens, "voice:listening")),
    ("voice-failed", |s| {
        voice(&s.lens, "voice:fail:Voice stopped: the speech helper quit.")
    }),
    ("still-failed", |s| still_failed(&s.lens)),
];

pub(crate) const INK_SCENES: Scenes<Ink> =
    &[("strip", |s| strip(&s.lens)), ("marks", |s| marks(&s.lens))];

pub(crate) const TILE_SCENES: Scenes<Tile> = &[("sent", |s| sent(&s.lens))];

pub(crate) const PILL_SCENES: Scenes<Pill> = &[
    ("starting", |s| play(&s.lens, &["key:voice"])),
    ("recording", |s| dictating(&s.lens)),
];

pub(crate) const PICKER_SCENES: Scenes<Picker> =
    &[("picking", |s| play(&s.lens, &["key:capture"]))];

fn play(lens: &Arc<Lens>, marks: &[&str]) {
    for mark in marks {
        lens.port()
            .scene_mark(mark)
            .unwrap_or_else(|error| panic!("{error}"));
    }
}

/// The capture key, then a drag across the picker from (200, 100) to (600, 400).
fn capture(lens: &Arc<Lens>) {
    capture_at(lens, [200., 100., 400., 300.]);
}

fn capture_at(lens: &Arc<Lens>, rect: [f64; 4]) {
    // Without the stand-in, the bar would list this machine's real sessions.
    assert!(
        std::env::var_os("ATTN_BIN").is_some(),
        "capture scenes list sessions through attn: set ATTN_BIN to tools/scenes/attn, as scripts/snap.sh does"
    );
    play(lens, &["key:capture"]);
    lens.pick(1, Some(rect)).expect("the fake has display 1");
}

/// A drag on the ink layer with the draw key down, in points on the display.
fn stroke(lens: &Arc<Lens>, tool: &str, from: (f32, f32), to: (f32, f32)) {
    play(lens, &["key:draw-down", tool]);
    let start = lens
        .begin_stroke()
        .expect("a capture is open with the draw key down");
    let points = (0..=8)
        .map(|step| {
            let f = step as f32 / 8.;
            (from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f)
        })
        .collect();
    lens.finish_stroke(start, points);
    play(lens, &["key:draw-up"]);
}

fn highlighted(lens: &Arc<Lens>) {
    capture(lens);
    stroke(lens, "key:4", (250., 150.), (350., 180.));
}

/// A stroke's still and two taken with the capture key, and a message typed.
fn three_stills(lens: &Arc<Lens>) {
    highlighted(lens);
    lens.set_message("hi!");
    play(lens, &["key:capture", "key:capture"]);
}

fn palette(lens: &Arc<Lens>) {
    capture(lens);
    lens.toggle_palette();
}

/// The session with the longest label as the target.
fn long_label(lens: &Arc<Lens>) {
    capture(lens);
    lens.choose_session(Some(6));
}

/// No token in the Keychain, so the bar offers to pair.
fn unpaired(lens: &Arc<Lens>) {
    lens.port().forget_token();
    play(lens, &["key:capture"]);
    lens.pick(1, Some([200., 100., 400., 300.]))
        .expect("the fake has display 1");
}

/// Voice turned on in a capture, then `status` from the speech helper.
fn voice(lens: &Arc<Lens>, status: &str) {
    capture(lens);
    play(lens, &["key:voice", status]);
}

/// A send whose still cannot be taken.
fn still_failed(lens: &Arc<Lens>) {
    capture_at(lens, [100., 80., 300., 200.]);
    play(lens, &["still:fail", "key:send"]);
}

/// The tool strip, shown while the draw key is down.
fn strip(lens: &Arc<Lens>) {
    capture_at(lens, [200., 100., 600., 450.]);
    play(lens, &["pointer:420:360", "key:draw-down"]);
}

/// One mark of each kind the strip draws.
fn marks(lens: &Arc<Lens>) {
    capture_at(lens, [200., 100., 600., 450.]);
    stroke(lens, "key:1", (420., 360.), (480., 400.));
    stroke(lens, "key:2", (300., 420.), (303., 480.));
    stroke(lens, "key:3", (250., 200.), (310., 240.));
    stroke(lens, "key:4", (600., 200.), (700., 220.));
    stroke(lens, "key:6", (650., 250.), (650., 250.));
    stroke(lens, "key:6", (700., 250.), (700., 250.));
}

/// The tile a sent capture shrinks to.
fn sent(lens: &Arc<Lens>) {
    capture_at(lens, [100., 100., 400., 300.]);
    stroke(lens, "key:1", (150., 150.), (250., 190.));
    play(lens, &["key:send"]);
}

/// Dictation with no capture open: the pill with two phrases heard.
fn dictating(lens: &Arc<Lens>) {
    play(
        lens,
        &[
            "key:voice",
            "voice:listening",
            "say:0.15:0.5:um so deploy",
            "say:0.5:0.8:to staging",
        ],
    );
}
